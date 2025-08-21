// src/handlers.rs
use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Error as SqlxError};
use sqlx::{Row};
use uuid::Uuid;

// Import types and functions from the auth module
use crate::{auth::{
    authorize_user, create_cookie_header, get_claims, get_cookie_from_claims, hash_password, AuthError, Claims, PartialClaims
}, AppState};




// ====================== 404 handler ======================
// pub async fn handle_404() -> Response {
//     (StatusCode::NOT_FOUND, "404 Not Found").into_response()
// }

// ====================== canvas stuff ======================

// A struct to represent a single canvas item in the response
#[derive(Debug, Serialize)]
pub struct CanvasListResponseItem {
    pub canvas_id: String,
    pub name: String,
    pub permission_level: String,
}

// The handler for the GET /api/canvases/list route
// This function will automatically have the Claims extractor run by Axum,
// ensuring the request is authenticated before it reaches this handler.
pub async fn get_canvas_list(
    State(state): State<AppState>,
    claims: Claims,
) -> impl IntoResponse {
    let pool = state.pool;

    // The claims already contain the canvas IDs and their permission levels.
    let canvas_permissions = claims.canvas_permissions;

    // Extract the canvas IDs from the claims' HashMap.
    let canvas_ids: Vec<&str> = canvas_permissions.keys().map(|id| id.as_str()).collect();
    
    // Check if there are any canvas IDs to query. If not, return an empty list immediately.
    if canvas_ids.is_empty() {
        return (StatusCode::OK, Json(Vec::<CanvasListResponseItem>::new())).into_response();
    }

    // The `sqlx` macro doesn't support dynamically-sized `IN` clauses directly,
    // so we need to build the query dynamically.
    let in_clause = format!(
        "('{}')",
        canvas_ids.join("','")
    );

    // SQL query to fetch the canvas name for each canvas_id
    let query_string = format!(
        "SELECT canvas_id, name FROM Canvas WHERE canvas_id IN {}",
        in_clause
    );

    let canvas_rows = match sqlx::query(&query_string)
        .fetch_all(&pool) // Fix: Changed &*pool to &pool. The pool is already a reference.
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("Database query failed: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to retrieve canvas list."}))
            ).into_response();
        }
    };
    
    // Build the final list of canvases to return.
    let mut response_list: Vec<CanvasListResponseItem> = Vec::new();

    for row in canvas_rows {
        let canvas_id: String = row.get("canvas_id");
        let name: String = row.get("name");
        
        // Find the permission level in the claims HashMap.
        // It's safe to unwrap here because the query was built from the keys of this map.
        let permission_level = canvas_permissions.get(&canvas_id).unwrap().clone();

        response_list.push(CanvasListResponseItem {
            canvas_id,
            name,
            permission_level,
        });
    }

    (
        StatusCode::OK,
        Json(response_list)
    ).into_response()
}


#[derive(Debug, Deserialize)]
pub struct CreateCanvasPayload {
    pub name: String,
}


pub async fn create_canvas(
    State(state): State<AppState>,
    claims: Claims, // User who is creating the canvas (owner)
    Json(payload): Json<CreateCanvasPayload>, // Name of the new canvas, now from JSON
) -> impl IntoResponse {

    let pool = state.pool;

    // 1. Validate payload
    if payload.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Canvas name cannot be empty."})),
        )
            .into_response();
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
    if let Err(e) = sqlx::query!(
        "INSERT INTO Canvas (canvas_id, name, owner_user_id, moderated, event_store) VALUES (?, ?, ?, ?, ?)",
        canvas_id,
        canvas_name,
        owner_user_id,
        false,
        ""
    )
    .execute(&mut *tx)
    .await
    {
        tx.rollback().await.ok();
        tracing::error!("Failed to create canvas: {:?}", e);
        return AuthError::DbError.into_response();
    }

    // 4. Insert into Canvas_Permissions table (set creator as Owner)
    if let Err(e) = sqlx::query!(
        "INSERT INTO Canvas_Permissions (user_id, canvas_id, permission_level) VALUES (?, ?, ?)",
        owner_user_id,
        canvas_id,
        "O"
    )
    .execute(&mut *tx)
    .await
    {
        tx.rollback().await.ok();
        tracing::error!("Failed to set owner permissions for canvas ID {}: {:?}", canvas_id, e);
        return AuthError::DbError.into_response();
    }

    // 5. Commit the transaction
    if let Err(e) = tx.commit().await {
        tracing::error!("Failed to commit transaction for canvas ID {}: {:?}", canvas_id, e);
        return AuthError::DbError.into_response();
    }

    // 6. Update canvas permissions in claims
    let mut updated_canvas_permissions = claims.canvas_permissions.clone();
    updated_canvas_permissions.insert(canvas_id.clone(), "O".to_string());

    // Step 1: Build new claims with updated permissions
    let updated_partial_claims = PartialClaims {
        email: claims.email.clone(),
        user_id: Some(claims.user_id),
        display_name: Some(claims.display_name.clone()),
        canvas_permissions: Some(updated_canvas_permissions),
        exp: claims.exp,
    };

    let updated_claims = match get_claims(&pool, updated_partial_claims).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to get updated claims after canvas creation: {:?}", e);
            return AuthError::DbError.into_response();
        }
    };
    
    // Step 2: Update the claims in the active WebSocket connections
    state.active_connections.update_user_claims(claims.user_id, updated_claims.clone()).await;

    // Step 3: Create new cookie from updated claims
    match get_cookie_from_claims(updated_claims).await {
        Ok(cookie) => {
            let headers = create_cookie_header(cookie);
            (
                StatusCode::CREATED,
                headers,
                Json(json!({
                    "message": "Canvas created successfully",
                    "canvas_id": canvas_id,
                })),
            )
                .into_response()
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
    State(state): State<AppState>,
    claims: Claims,
    Json(payload): Json<UpdateUserPayload>, // Changed to accept a JSON payload
) -> impl IntoResponse {

    let pool = state.pool;

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
        match sqlx::query!(
            "SELECT user_id FROM users WHERE email = ? AND user_id != ?",
            new_email,
            claims.user_id
        )
        .fetch_optional(&mut *tx)
        .await
        {
            Ok(Some(_)) => {
                tx.rollback().await.ok();
                tracing::info!("Profile update failed: Email '{}' already taken by another user.", new_email);
                return AuthError::UserExists.into_response();
            }
            Ok(None) => {
                if let Err(e) = sqlx::query!(
                    "UPDATE users SET email = ? WHERE user_id = ?",
                    new_email,
                    claims.user_id
                )
                .execute(&mut *tx)
                .await
                {
                    tx.rollback().await.ok();
                    tracing::error!("Failed to update email for user {}: {:?}", claims.user_id, e);
                    return AuthError::DbError.into_response();
                }
                tracing::info!("User {} (ID: {}) updated email to '{}'.", claims.email, claims.user_id, new_email);
                updated_email = new_email;
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
        if let Err(e) = sqlx::query!(
            "UPDATE users SET display_name = ? WHERE user_id = ?",
            new_display_name,
            claims.user_id
        )
        .execute(&mut *tx)
        .await
        {
            tx.rollback().await.ok();
            tracing::error!("Failed to update display name for user {}: {:?}", claims.user_id, e);
            return AuthError::DbError.into_response();
        }
        tracing::info!("User {} (ID: {}) updated display name to '{}'.", claims.email, claims.user_id, new_display_name);
        updated_display_name = new_display_name;
    }

    match tx.commit().await {
        Ok(_) => tracing::debug!("Transaction committed for user {}", claims.user_id),
        Err(e) => {
            tracing::error!("Failed to commit transaction for user {}: {:?}", claims.user_id, e);
            return AuthError::DbError.into_response();
        }
    }

    // Step 1: Build new partial claims with updated info
    let updated_partial_claims = PartialClaims {
        email: updated_email.clone(),
        display_name: Some(updated_display_name.clone()),
        user_id: Some(claims.user_id),
        canvas_permissions: Some(claims.canvas_permissions.clone()),
        exp: claims.exp,
    };

    // Step 2: Fetch full updated claims from DB
    let updated_claims = match get_claims(&pool, updated_partial_claims).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to get updated claims after profile update: {:?}", e);
            return AuthError::DbError.into_response();
        }
    };

    // Step 3: Update claims in active WebSocket connections
    state.active_connections.update_user_claims(claims.user_id, updated_claims.clone()).await;

    // Step 4: Create new cookie from updated claims
    match get_cookie_from_claims(updated_claims).await {
        Ok(cookie) => {
            let headers = create_cookie_header(cookie);
            (
                StatusCode::OK,
                headers,
                Json(json!({"message": "Profile updated successfully."})),
            )
                .into_response()
        }
        Err(e) => e.into_response(),
    }
}




// ====================== login logout ======================

pub async fn logout() -> impl IntoResponse {
    let mut headers = HeaderMap::new();

    // Invalidate the cookie
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "auth_token=; HttpOnly; Path=/; Max-Age=0; SameSite=Strict"
        ),
    );

    // Return a success status code and a simple JSON message
    (StatusCode::OK, headers, Json(json!({"message": "Successfully logged out"})))
}




#[derive(Debug, Deserialize)]
pub struct LoginPayload {
    pub email: String,
    pub password: String,
}

pub async fn login(
    State(state): State<AppState>,
    // Change from `Form(payload)` to `Json(payload)`
    Json(payload): Json<LoginPayload>,
) -> impl IntoResponse {

    tracing::debug!("login called: user {}; pwd {}", payload.email, payload.password);
    
    match authorize_user(&state.pool, &payload.email, &payload.password).await {
        Ok(cookie) => {
            let headers = create_cookie_header(cookie);
            (StatusCode::OK, headers, Json(json!({"message": "Login successful"}))).into_response()
        }
        Err(e) => {
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
    State(state): State<AppState>,
    Json(payload): Json<RegisterPayload>,
) -> impl IntoResponse {
    if payload.email.is_empty() || payload.password.is_empty() || payload.display_name.is_empty() {
        return AuthError::MissingCredentials.into_response();
    }

    let password_hash = match hash_password(&payload.password) {
        Ok(hash) => hash,
        Err(_) => return AuthError::PasswordHashingFailed.into_response(),
    };

    match sqlx::query!(
        "INSERT INTO users (email, password_hash, display_name) VALUES (?, ?, ?)",
        payload.email,
        password_hash,
        payload.display_name
    )
    .execute(&state.pool)
    .await
    {
        Ok(_) => {
            tracing::info!("User {} registered successfully.", payload.email);

            // Fetch full claims from DB for this user by email
            let claims = match get_claims(&state.pool, PartialClaims {
                email: payload.email.clone(),
                user_id: None,
                display_name: Some(payload.display_name.clone()),
                ..PartialClaims::default()
            }).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("Failed to fetch claims after registration: {:?}", e);
                    return AuthError::DbError.into_response();
                }
            };

            // Generate the cookie string from full claims
            let cookie_str = match get_cookie_from_claims(claims).await {
                Ok(cookie) => cookie,
                Err(e) => {
                    tracing::error!("Failed to create cookie after registration: {:?}", e);
                    return AuthError::TokenCreation.into_response();
                }
            };

            // Build cookie header
            let headers = create_cookie_header(cookie_str);

            // Return success with the cookie header, logging the user in automatically
            (StatusCode::CREATED, headers, Json(json!({"message": "Registration successful"}))).into_response()
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
