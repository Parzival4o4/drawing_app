// src/handlers.rs
use axum::{
    extract::{State, Form},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::{sqlite::{SqlitePool, }};
use sqlx::{Error as SqlxError, query};
use uuid::Uuid;
use std::{fs};

// Import types and functions from the auth module
use crate::auth::{
    authorize_user, create_cookie, create_cookie_header, hash_password, AuthError, Claims, PartialClaims
};




// ====================== 404 handler ======================
pub async fn handle_404() -> Response {
    (StatusCode::NOT_FOUND, "404 Not Found").into_response()
}

// ====================== canvas stuff ======================
#[derive(Debug, Deserialize)]
pub struct CreateCanvasPayload {
    pub name: String,
}


pub async fn create_canvas(
    State(pool): State<SqlitePool>,
    claims: Claims, // User who is creating the canvas (owner)
    Form(payload): Form<CreateCanvasPayload>, // Name of the new canvas
) -> impl IntoResponse {
    // 1. Validate payload
    if payload.name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"error": "Canvas name cannot be empty."}))).into_response();
    }

    // 2. Generate a unique canvas_id
    let canvas_id = Uuid::new_v4().to_string();
    let owner_user_id = claims.user_id;
    let canvas_name = payload.name.trim().to_string();

    let mut tx = match pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to begin transaction for new canvas: {:?}", e);
            return AuthError::DbError.into_response();
        }
    };

    // 3. Insert into Canvas table
    match query!(
        "INSERT INTO Canvas (canvas_id, name, owner_user_id, moderated, event_store) VALUES (?, ?, ?, ?, ?)",
        canvas_id,
        canvas_name,
        owner_user_id,
        false, // Default: not moderated
        ""     // Default: empty event_store
    )
    .execute(&mut *tx)
    .await
    {
        Ok(_) => {
            tracing::info!("Canvas '{}' (ID: {}) created by user ID: {}.", canvas_name, canvas_id, owner_user_id);
        }
        Err(e) => {
            tx.rollback().await.ok();
            tracing::error!("Failed to create canvas: {:?}", e);
            return AuthError::DbError.into_response();
        }
    }

    // 4. Insert into Canvas_Permissions table (set creator as Owner)
    match query!(
        "INSERT INTO Canvas_Permissions (user_id, canvas_id, permission_level) VALUES (?, ?, ?)",
        owner_user_id,
        canvas_id,
        "O" // 'O' for Owner
    )
    .execute(&mut *tx)
    .await
    {
        Ok(_) => {
            tracing::info!("Permissions set for owner (ID: {}) on canvas ID: {}.", owner_user_id, canvas_id);
        }
        Err(e) => {
            tx.rollback().await.ok();
            tracing::error!("Failed to set owner permissions for canvas ID {}: {:?}", canvas_id, e);
            return AuthError::DbError.into_response();
        }
    }

    // 5. Commit the transaction
    match tx.commit().await {
        Ok(_) => {
            tracing::info!("Transaction committed for creating canvas ID: {}", canvas_id);
        }
        Err(e) => {
            tracing::error!("Failed to commit transaction for canvas ID {}: {:?}", canvas_id, e);
            return AuthError::DbError.into_response();
        }
    }
    
    // 6. Create a new cookie with the updated permissions
    let mut updated_canvas_permissions = claims.canvas_permissions.clone();
    updated_canvas_permissions.insert(canvas_id.clone(), "O".to_string()); // Add the new canvas permission

    match create_cookie(
        &pool,
        PartialClaims {
            email: claims.email.clone(),
            user_id: Some(claims.user_id),
            display_name: Some(claims.display_name.clone()),
            canvas_permissions: Some(updated_canvas_permissions), // Pass the updated list
        },
    )
    .await {
        Ok(cookie) => {
            let headers = create_cookie_header(cookie);
            (StatusCode::CREATED, headers, Json(json!({
                "message": "Canvas created successfully",
                "canvas_id": canvas_id,
            }))).into_response()
        }
        Err(e) => e.into_response(),
    }
}

// ====================== User Profile ======================

pub async fn get_user_info(
    claims: Claims, // The Claims extractor will get this from the request extensions
) -> impl IntoResponse {
    Json(json!({
        "user_id": claims.user_id,
        "email": claims.email,
        "display_name": claims.display_name,
    }))
}


// Handler for updating a user's profile information.
#[derive(Debug, Deserialize)]
pub struct UpdateUserPayload {
    pub email: Option<String>,
    pub display_name: Option<String>,
}

pub async fn update_profile(
    State(pool): State<SqlitePool>,
    claims: Claims, // Extracted by auth_middleware and FromRequestParts
    Form(payload): Form<UpdateUserPayload>, // New payload for updates
) -> impl IntoResponse {
    if payload.email.is_none() && payload.display_name.is_none() {
        tracing::debug!("No fields provided for profile update for user {}", claims.user_id);
        return (StatusCode::NO_CONTENT, Json(json!({"message": "No fields to update"}))).into_response();
    }

    let mut tx = match pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to begin transaction for profile update: {:?}", e);
            return AuthError::DbError.into_response();
        }
    };

    let mut updated_email = claims.email.clone();
    let mut updated_display_name = claims.display_name.clone();

    if let Some(new_email) = payload.email {
        if new_email.is_empty() {
             tx.rollback().await.ok();
             return (StatusCode::BAD_REQUEST, Json(json!({"error": "Email cannot be empty."}))).into_response();
        }
        match query!("SELECT user_id FROM users WHERE email = ? AND user_id != ?", new_email, claims.user_id)
            .fetch_optional(&mut *tx)
            .await
        {
            Ok(Some(_)) => {
                tx.rollback().await.ok();
                tracing::info!("Profile update failed: Email '{}' already taken by another user.", new_email);
                return AuthError::UserExists.into_response();
            }
            Ok(None) => {
                match query!("UPDATE users SET email = ? WHERE user_id = ?", new_email, claims.user_id)
                    .execute(&mut *tx)
                    .await
                {
                    Ok(_) => {
                        tracing::info!("User {} (ID: {}) updated email to '{}'.", claims.email, claims.user_id, new_email);
                        updated_email = new_email;
                    }
                    Err(e) => {
                        tx.rollback().await.ok();
                        tracing::error!("Failed to update email for user {}: {:?}", claims.user_id, e);
                        return AuthError::DbError.into_response();
                    }
                }
            }
            Err(e) => {
                tx.rollback().await.ok();
                tracing::error!("DB error checking email uniqueness for user {}: {:?}", claims.user_id, e);
                return AuthError::DbError.into_response();
            }
        }
    }

    if let Some(new_display_name) = payload.display_name {
        if new_display_name.is_empty() {
            tx.rollback().await.ok();
            return (StatusCode::BAD_REQUEST, Json(json!({"error": "Display name cannot be empty."}))).into_response();
        }
        match query!("UPDATE users SET display_name = ? WHERE user_id = ?", new_display_name, claims.user_id)
            .execute(&mut *tx)
            .await
        {
            Ok(_) => {
                tracing::info!("User {} (ID: {}) updated display name to '{}'.", claims.email, claims.user_id, new_display_name);
                updated_display_name = new_display_name;
            }
            Err(e) => {
                tx.rollback().await.ok();
                tracing::error!("Failed to update display name for user {}: {:?}", claims.user_id, e);
                return AuthError::DbError.into_response();
            }
        }
    }

    match tx.commit().await {
        Ok(_) => tracing::debug!("Transaction committed for user {}", claims.user_id),
        Err(e) => {
            tracing::error!("Failed to commit transaction for user {}: {:?}", claims.user_id, e);
            return AuthError::DbError.into_response();
        }
    }

    // After a successful update, create a new cookie with the updated claims
    // and return it with a redirect.
    match create_cookie(
        &pool,
        PartialClaims {
            email: updated_email,
            display_name: Some(updated_display_name),
            user_id: Some(claims.user_id),
            canvas_permissions: Some(claims.canvas_permissions), // Keep the existing permissions
        },
    )
    .await {
        Ok(cookie) => {
            let headers = create_cookie_header(cookie);
            // Changed from Redirect to a success message with headers.
            (StatusCode::OK, headers, Json(json!({"message": "Profile updated successfully."}))).into_response()
        }
        Err(e) => e.into_response(),
    }
}



// ====================== login logout ======================

pub async fn logout() -> impl IntoResponse {
    let mut headers = axum::http::HeaderMap::new();

    headers.insert(
        axum::http::header::SET_COOKIE,
        axum::http::HeaderValue::from_static(
            "auth_token=; HttpOnly; Path=/; Max-Age=0; SameSite=Strict"
        ),
    );

    (headers, Redirect::to("/login")).into_response()
}



#[derive(Debug, Deserialize)]
pub struct LoginPayload {
    pub email: String,
    pub password: String,
}

pub async fn login(
    State(pool): State<SqlitePool>,
    Form(payload): Form<LoginPayload>,
) -> impl IntoResponse {
    // Attempt to authorize the user and get the cookie string.
    match authorize_user(&pool, &payload.email, &payload.password).await {
        Ok(cookie) => {
            // If authorization is successful, create the headers with the cookie.
            let headers = create_cookie_header(cookie);
            
            // Return the headers along with a redirect to the home page.
            (headers, Redirect::to("/")).into_response()
        }
        Err(e) => {
            // If there's an error, convert it into an appropriate HTTP response.
            e.into_response()
        }
    }
}



// Handler for user registration.
#[derive(Debug, Deserialize)]
pub struct RegisterPayload {
    pub email: String,
    pub password: String,
    pub display_name: String,
}

pub async fn register(
    State(pool): State<SqlitePool>,
    Form(payload): Form<RegisterPayload>
) -> impl IntoResponse {
    if payload.email.is_empty() || payload.password.is_empty() || payload.display_name.is_empty() {
        return AuthError::MissingCredentials.into_response();
    }

    let password_hash = match hash_password(&payload.password) {
        Ok(hash) => hash,
        Err(_) => return AuthError::PasswordHashingFailed.into_response(),
    };

    match query!(
        "INSERT INTO users (email, password_hash, display_name) VALUES (?, ?, ?)",
        payload.email,
        password_hash,
        payload.display_name
    )
    .execute(&pool)
    .await
    {
        Ok(_) => {
            tracing::info!("User {} registered successfully.", payload.email);
            
            // directly use create_cookie and create_cookie_header
            // after a successful registration.
            match create_cookie(
                &pool,
                PartialClaims {
                    email: payload.email.clone(),
                    user_id: None, // Let create_cookie handle the lookup
                    display_name: Some(payload.display_name),
                    ..PartialClaims::default()
                },
            )
            .await
            {
                Ok(cookie) => {
                    let headers = create_cookie_header(cookie);
                    (headers, Redirect::to("/")).into_response()
                }
                Err(e) => e.into_response(),
            }
        }
        Err(SqlxError::Database(db_error)) if db_error.code() == Some("2067".into()) => {
            tracing::info!("Registration failed: User {} already exists.", payload.email);
            AuthError::UserExists.into_response()
        }
        Err(e) => {
            tracing::error!("Failed to register user {}: {:?}", payload.email, e);
            AuthError::DbError.into_response()
        }
    }
}




pub async fn login_page() -> impl IntoResponse {
    match fs::read_to_string("login.html") {
        Ok(contents) => Html(contents).into_response(),
        Err(_) => {
            tracing::error!("login.html not found!");
            Html("<h1>Login page not found</h1>").into_response()
        },
    }
}

pub async fn register_page() -> impl IntoResponse {
    match fs:: read_to_string("register.html") {
        Ok(contents) => Html(contents).into_response(),
        Err(_) => {
            tracing::error!("register.html not found!");
            Html("<h1>Register page not found</h1>").into_response()
        },
    }
}