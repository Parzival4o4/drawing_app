// src/handlers.rs
use axum::{
    extract::{State, Form},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::{query_as, sqlite::{SqlitePool, SqliteRow}};
use sqlx::{Error as SqlxError, Row, query};
use uuid::Uuid;
use std::{collections::HashMap, fs};

// Import types and functions from the auth module
use crate::auth::{
    AuthError, Claims,
    hash_password, verify_password, jwt_response,
};
use crate::KEYS; // Import KEYS from the main crate




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
            // This error case is less likely if canvas insert succeeded and primary key is composite.
            // But handle it for completeness.
            tracing::error!("Failed to set owner permissions for canvas ID {}: {:?}", canvas_id, e);
            return AuthError::DbError.into_response();
        }
    }

    // 5. Commit the transaction
    match tx.commit().await {
        Ok(_) => {
            tracing::info!("Transaction committed for creating canvas ID: {}", canvas_id);
            (StatusCode::CREATED, Json(json!({
                "message": "Canvas created successfully",
                "canvas_id": canvas_id,
                "name": canvas_name,
                "owner_user_id": owner_user_id,
                "permission_level": "O" // Indicate the user's permission
            }))).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to commit transaction for canvas ID {}: {:?}", canvas_id, e);
            AuthError::DbError.into_response()
        }
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

    // Start with the current claims' email and display_name,
    // which will be updated if the payload contains new values.
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

    // NEW: Construct new_claims by cloning 'claims' and overriding 'email' and 'display_name'
    let new_claims = Claims {
        email: updated_email,
        display_name: updated_display_name,
        // All other fields (user_id, exp, canvas_permissions) are copied from the original 'claims'
        ..claims
    };

    jwt_response(new_claims)
}


// ====================== login logout ======================

pub async fn logout() -> impl IntoResponse {
    let mut headers = axum::http::HeaderMap::new();

    headers.insert(
        axum::http::header::SET_COOKIE,
        axum::http::HeaderValue::from_static("auth_token=; HttpOnly; Path=/; Max-Age=0"),
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
    Form(payload): Form<LoginPayload>
) -> impl IntoResponse {
    match authorize_user(&pool, &payload).await {
        Ok(claims) => jwt_response(claims), // Pass the Claims to jwt_response
        Err(e) => e.into_response(),
    }
}


#[derive(Debug, Deserialize)]
pub struct AuthPayload {
    pub email: String,
    pub password: String,
    pub display_name: String,
}

pub async fn register(
    State(pool): State<SqlitePool>,
    Form(payload): Form<AuthPayload>
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
            match authorize_user(&pool, &payload).await {
                Ok(claims) => jwt_response(claims), // Pass the Claims to jwt_response
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


pub trait AuthCommon {
    fn email(&self) -> &str;
    fn password(&self) -> &str;
}

impl AuthCommon for AuthPayload {
    fn email(&self) -> &str { &self.email }
    fn password(&self) -> &str { &self.password }
}

impl AuthCommon for LoginPayload {
    fn email(&self) -> &str { &self.email }
    fn password(&self) -> &str { &self.password }
}


pub async fn authorize_user<T>(
    pool: &SqlitePool,
    payload: &T
) -> Result<Claims, AuthError> // <--- CHANGE: Now returns Claims on success
where
    T: AuthCommon + Send + Sync + 'static,
{
    if payload.email().is_empty() || payload.password().is_empty() {
        return Err(AuthError::MissingCredentials);
    }

    let user_row: Option<SqliteRow> = query(
        "SELECT user_id, email, password_hash, display_name FROM users WHERE email = ?"
    )
    .bind(payload.email())
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::error!("Database query error during authorization (user fetch): {:?}", e);
        AuthError::DbError
    })?;

    match user_row {
        Some(row) => {
            let stored_password_hash: String = row.try_get("password_hash")
                .map_err(|e| {
                    tracing::error!("Failed to get password_hash from row: {:?}", e);
                    AuthError::DbError
                })?;

            if verify_password(payload.password(), &stored_password_hash).map_err(|_| AuthError::WrongCredentials)? {
                let user_id: i64 = row.try_get("user_id")
                    .map_err(|e| {
                        tracing::error!("Failed to get user_id from row: {:?}", e);
                        AuthError::DbError
                    })?;
                let display_name: String = row.try_get("display_name")
                    .map_err(|e| {
                        tracing::error!("Failed to get display_name from row: {:?}", e);
                        AuthError::DbError
                    })?;

                // Define a temporary struct to hold the query result for permissions
                #[derive(sqlx::FromRow)]
                struct UserCanvasPermission {
                    canvas_id: String,
                    permission_level: String,
                }

                let user_permissions: Vec<UserCanvasPermission> = query_as!(
                    UserCanvasPermission,
                    "SELECT canvas_id, permission_level FROM Canvas_Permissions WHERE user_id = ?",
                    user_id
                )
                .fetch_all(pool)
                .await
                .map_err(|e| {
                    tracing::error!("Database query error fetching canvas permissions for user {}: {:?}", user_id, e);
                    AuthError::DbError
                })?;

                // Convert Vec<UserCanvasPermission> into HashMap<String, String>
                let canvas_permissions: HashMap<String, String> = user_permissions
                    .into_iter()
                    .map(|p| (p.canvas_id, p.permission_level))
                    .collect();

                let claims = Claims {
                    user_id,
                    email: payload.email().to_string(),
                    display_name,
                    exp: 2_000_000_000,
                    canvas_permissions,
                };

                tracing::info!("Authorized user: {} (ID: {}) with {} canvas permissions", claims.email, claims.user_id, claims.canvas_permissions.len());
                Ok(claims) // <--- CHANGE: Now return the Claims struct
            } else {
                tracing::info!("Authorization failed: Wrong password for user {}", payload.email());
                Err(AuthError::WrongCredentials)
            }
        }
        None => {
            tracing::info!("Authorization failed: User {} not found.", payload.email());
            Err(AuthError::WrongCredentials)
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