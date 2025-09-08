use std::{collections::HashMap, path::PathBuf};
use tokio::fs; 

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{query, Error as SqlxError, SqlitePool};
use sqlx::{Row};
use uuid::Uuid;

// Import types and functions from the auth module
use crate::{auth::{
    authorize_user, create_cookie_header, get_claims, get_cookie_from_claims, hash_password, AuthError, Claims, PartialClaims
}, AppState};



// ====================== canvas stuff ======================

// A struct to represent a single canvas item in the response
#[derive(Debug, Serialize)]
pub struct CanvasListResponseItem {
    pub canvas_id: String,
    pub name: String,
    pub permission_level: String,
}

// The handler for the GET /api/canvases/list route
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
        .fetch_all(&pool) 
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
    claims: Claims,
    Json(payload): Json<CreateCanvasPayload>,
) -> impl IntoResponse {

    let pool = state.pool;

    if payload.name.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Canvas name cannot be empty."})),
        ).into_response();
    }

    let canvas_id = Uuid::new_v4().to_string();
    let owner_user_id = claims.user_id;
    let canvas_name = payload.name.trim().to_string();
    
    let data_dir = PathBuf::from("data");
    let canvases_dir = data_dir.join("canvases");
    let file_path = canvases_dir.join(format!("{}.jsonl", canvas_id));

    if let Err(e) = fs::create_dir_all(&canvases_dir).await {
        tracing::error!("Failed to create canvases directory: {:?}", e);
        return AuthError::DbError.into_response();
    }

    if let Err(e) = fs::File::create(&file_path).await {
        tracing::error!("Failed to create event file at {}: {:?}", file_path.display(), e);
        return AuthError::DbError.into_response();
    }
    
    let mut tx = match pool.begin().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to begin transaction for new canvas: {:?}", e);
            return AuthError::DbError.into_response();
        }
    };

    // Fix for the temporary value dropped while borrowed error
    let file_path_str = file_path.to_str().unwrap_or("");

    if let Err(e) = sqlx::query!(
        "INSERT INTO Canvas (canvas_id, name, owner_user_id, moderated, event_file_path) VALUES (?, ?, ?, ?, ?)",
        canvas_id,
        canvas_name,
        owner_user_id,
        false,
        file_path_str // Use the new variable here
    )
    .execute(&mut *tx)
    .await
    {
        tx.rollback().await.ok();
        tracing::error!("Failed to create canvas: {:?}", e);
        return AuthError::DbError.into_response();
    }

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

    if let Err(e) = tx.commit().await {
        tracing::error!("Failed to commit transaction for canvas ID {}: {:?}", canvas_id, e);
        return AuthError::DbError.into_response();
    }
    
    let mut updated_canvas_permissions = claims.canvas_permissions.clone();
    updated_canvas_permissions.insert(canvas_id.clone(), "O".to_string());

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
    
    state.socket_claims_manager.update_claims(claims.user_id, updated_claims.clone()).await;

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
            ).into_response()
        }
        Err(e) => e.into_response(),
    }
}

// ====================== Permissions ======================


#[derive(Deserialize)]
pub struct UpdatePermissionRequest {
    pub user_id: i64,
    pub permission: String,
}

#[derive(Serialize)]
struct GenericResponse {
    message: String,
}
// New helper function to remove a user's permissions from a canvas
async fn remove_user_canvas_permissions(
    pool: &SqlitePool,
    canvas_id: &str,
    user_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM Canvas_Permissions WHERE canvas_id = ? AND user_id = ?",
        canvas_id,
        user_id
    )
    .execute(pool)
    .await?;

    Ok(())
}


pub async fn update_canvas_permissions(
    claims: Claims,
    State(state): State<AppState>,
    Path(canvas_id): Path<String>,
    Json(payload): Json<UpdatePermissionRequest>,
) -> impl IntoResponse {
    // 1. Get acting user's permission
    let acting_user_permission = claims.canvas_permissions.get(&canvas_id);

    // 2. Prevent self-modification
    if claims.user_id == payload.user_id {
        tracing::warn!(
            "User {} tried to change their own permissions on canvas {}.",
            claims.user_id, canvas_id
        );
        return (
            axum::http::StatusCode::FORBIDDEN,
            Json(GenericResponse {
                message: "Cannot change your own permissions.".to_string(),
            }),
        )
            .into_response();
    }

    // 3. Get target user's current permission
    let target_user_permission =
        get_user_canvas_permissions_from_db(&state.pool, &canvas_id, payload.user_id).await;

    // 4. Disallow modifying the owner
    if let Some(target_permission) = &target_user_permission {
        if target_permission == "O" {
            tracing::warn!(
                "User {} tried to change the owner's permissions on canvas {}.",
                claims.user_id, canvas_id
            );
            return (
                axum::http::StatusCode::FORBIDDEN,
                Json(GenericResponse {
                    message: "Cannot change the owner's permissions.".to_string(),
                }),
            )
                .into_response();
        }
    }

    // 5. Permission check
    let can_change = match acting_user_permission.map(|p| p.as_str()) {
        Some("C") | Some("O") => true,
        Some("M") => {
            !matches!(payload.permission.as_str(), "C" | "M")
                && !matches!(
                    target_user_permission.as_deref(),
                    Some("C") | Some("O") | Some("M")
                )
        }
        _ => {
            tracing::warn!(
                "User {} does not have sufficient permission to change permissions on canvas {}.",
                claims.user_id,
                canvas_id
            );
            return (
                axum::http::StatusCode::FORBIDDEN,
                Json(GenericResponse {
                    message: "Insufficient permissions.".to_string(),
                }),
            )
                .into_response();
        }
    };

    if !can_change {
        tracing::warn!(
            "Permission check failed for user {} on canvas {}. New permission: {}, Target current: {:?}",
            claims.user_id,
            canvas_id,
            payload.permission,
            target_user_permission
        );
        return (
            axum::http::StatusCode::FORBIDDEN,
            Json(GenericResponse {
                message: "Insufficient permissions for this action.".to_string(),
            }),
        )
            .into_response();
    }

    // 6. Update/remove DB permissions
    let mut removed = false;
    if payload.permission.is_empty() {
        match remove_user_canvas_permissions(&state.pool, &canvas_id, payload.user_id).await {
            Ok(_) => {
                tracing::info!(
                    "Permissions for user {} on canvas {} removed.",
                    payload.user_id,
                    canvas_id
                );
                removed = true;
            }
            Err(e) => {
                tracing::error!(
                    "Failed to remove permissions for user {} on canvas {}: {}",
                    payload.user_id,
                    canvas_id,
                    e
                );
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(GenericResponse {
                        message: "Failed to remove permissions.".to_string(),
                    }),
                )
                    .into_response();
            }
        }
    } else {
        match update_user_canvas_permissions(
            &state.pool,
            &canvas_id,
            payload.user_id,
            &payload.permission,
        )
        .await
        {
            Ok(_) => {
                tracing::info!(
                    "Permissions for user {} on canvas {} updated to {}.",
                    payload.user_id,
                    canvas_id,
                    payload.permission
                );
            }
            Err(e) => {
                tracing::error!(
                    "Failed to update permissions for user {} on canvas {}: {}",
                    payload.user_id,
                    canvas_id,
                    e
                );
                return (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    Json(GenericResponse {
                        message: "Failed to update permissions.".to_string(),
                    }),
                )
                    .into_response();
            }
        }
    }

    // 7. Mark user for refresh
    state.permission_refresh_list.mark_user_for_refresh(payload.user_id).await;

    // 8. Refresh claims in SocketClaimsManager
    state
        .socket_claims_manager
        .update_permissions(&state, payload.user_id)
        .await;

    // 9. Unregister only if permissions were removed
    if removed {
        state
            .canvas_manager
            .unregister_user(&canvas_id, payload.user_id)
            .await;
    }

    // 10. Return success
    (
        axum::http::StatusCode::OK,
        Json(GenericResponse {
            message: "Permissions updated successfully.".to_string(),
        }),
    )
        .into_response()
}




pub async fn get_user_canvas_permissions_from_db(
    pool: &SqlitePool,
    canvas_id: &str,
    user_id: i64,
) -> Option<String> {
    let result = query!(
        "SELECT permission_level FROM Canvas_Permissions WHERE canvas_id = ? AND user_id = ?",
        canvas_id,
        user_id
    )
    .fetch_optional(pool)
    .await;

    match result {
        Ok(record) => record.map(|r| r.permission_level),
        Err(e) => {
            tracing::error!("Failed to fetch user permissions from DB: {:?}", e);
            None
        }
    }
}

pub async fn update_user_canvas_permissions(
    pool: &SqlitePool,
    canvas_id: &str,
    user_id: i64,
    permission_level: &str,
) -> Result<(), SqlxError> { // Corrected function signature
    query!(
        "INSERT INTO Canvas_Permissions (user_id, canvas_id, permission_level)
         VALUES (?, ?, ?)
         ON CONFLICT(user_id, canvas_id) DO UPDATE SET permission_level = excluded.permission_level",
        user_id,
        canvas_id,
        permission_level
    )
    .execute(pool)
    .await?;

    Ok(())
}



// A new struct to represent a user for the JSON response
#[derive(Debug, Serialize, Deserialize)]
pub struct CanvasUser {
    pub user_id: i64,
    pub display_name: String,
}

/// Retrieves all users and their permissions for a given canvas.
pub async fn get_canvas_permissions(
    State(state): State<AppState>,
    Path(canvas_id): Path<String>,
) -> Result<Json<HashMap<String, Vec<CanvasUser>>>, StatusCode> {
    // Perform a SQL query to get all users and their permissions for the canvas
    let rows = sqlx::query!(
        r#"
        SELECT
            T1.permission_level,
            T2.user_id,
            T2.display_name
        FROM
            Canvas_Permissions AS T1
        JOIN
            users AS T2
        ON
            T1.user_id = T2.user_id
        WHERE
            T1.canvas_id = ?
        "#,
        canvas_id
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Database query error fetching canvas permissions: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Use a HashMap to group users by their permission level
    let mut permissions_map: HashMap<String, Vec<CanvasUser>> = HashMap::new();

    for row in rows {
        let user = CanvasUser {
            user_id: row.user_id,
            display_name: row.display_name,
        };

        // Get the vector for the current permission level, or create a new one if it doesn't exist.
        let users_for_permission = permissions_map.entry(row.permission_level).or_insert_with(Vec::new);

        // Add the user to the vector
        users_for_permission.push(user);
    }

    Ok(Json(permissions_map))
}


// ====================== User Profile ======================

pub async fn get_user_info(
    claims: Claims, 
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
    Json(payload): Json<UpdateUserPayload>, 
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
    state.socket_claims_manager.update_claims(claims.user_id, updated_claims.clone()).await;

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
