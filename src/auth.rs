use std::collections::HashMap;

// src/auth.rs
use axum::{
    extract::{FromRequestParts },
    http::{header, request::Parts, HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    Json,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2, PasswordHash, PasswordVerifier,
};
use sqlx::SqlitePool;

// Import the LazyLock from the main crate
use crate::KEYS;

// ───── 1. Types and their impls ────────────
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub user_id: i64,
    pub email: String,
    pub display_name: String,
    pub exp: usize,
    pub canvas_permissions: HashMap<String, String>,
}

impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = Redirect;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Attempt to get claims from request extensions first (if already authenticated by middleware)
        if let Some(claims) = parts.extensions.get::<Claims>() {
            return Ok(claims.clone());
        }

        let cookies = parts
            .headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");

        let token = cookies
            .split(';')
            .find_map(|cookie| {
                let mut split = cookie.trim().splitn(2, '=');
                match (split.next(), split.next()) {
                    (Some("auth_token"), Some(value)) => Some(value),
                    _ => None,
                }
            });

        let token = match token {
            Some(t) => t,
            None => return Err(Redirect::to("/login")),
        };

        let decoded = decode::<Claims>(token, &KEYS.decoding, &Validation::default())
            .map_err(|_| Redirect::to("/login"))?;

        Ok(decoded.claims)
    }
}

pub struct Keys {
    pub encoding: EncodingKey,
    pub decoding: DecodingKey,
}

impl Keys {
    pub fn new(secret: &[u8]) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
        }
    }
}


#[derive(Debug)]
pub enum AuthError {
    WrongCredentials,
    MissingCredentials,
    UserExists,
    TokenCreation,
    PasswordHashingFailed,
    DbError,
    UserInfoNotFound,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AuthError::WrongCredentials => (StatusCode::UNAUTHORIZED, "Wrong credentials"),
            AuthError::MissingCredentials => (StatusCode::BAD_REQUEST, "Missing credentials"),
            AuthError::UserExists => (StatusCode::CONFLICT, "User already exists"),
            AuthError::TokenCreation => (StatusCode::INTERNAL_SERVER_ERROR, "Token creation error"),
            AuthError::PasswordHashingFailed => (StatusCode::INTERNAL_SERVER_ERROR, "Password hashing failed"),
            AuthError::DbError => (StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
            AuthError::UserInfoNotFound => (StatusCode::NOT_FOUND, "User information not found"),
        };
        let body = Json(json!({ "error": error_message }));
        (status, body).into_response()
    }
}

// ───── 2. Middleware ───────────────────────
// middleware that dose not require access to internal state 
pub async fn auth_middleware(
    claims_result: Result<Claims, Redirect>,
    request: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    match claims_result {
        Ok(claims) => {
            tracing::debug!("Authenticated user: {} (ID: {}) accessing {:?}", claims.email, claims.user_id, request.uri());
            let mut request = request;
            request.extensions_mut().insert(claims);
            next.run(request).await
        }
        Err(redirect) => {
            tracing::debug!("Unauthenticated request for {:?}, redirecting to /login", request.uri());
            redirect.into_response()
        }
    }
}

// ───── 3. Utilities ────────────────────────
// Password Hashing Helper
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2.hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
}

// Password Verification Helper
pub fn verify_password(password: &str, hashed_password: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed_hash = PasswordHash::new(hashed_password)?;
    Ok(Argon2::default().verify_password(password.as_bytes(), &parsed_hash).is_ok())
}

// Shared cookie + redirect response builder
pub fn jwt_response(claims: Claims) -> Response {
    let token = match encode(&jsonwebtoken::Header::default(), &claims, &KEYS.encoding) {
        Ok(token) => token,
        Err(e) => {
            tracing::error!("Failed to create token in jwt_response: {:?}", e);
            // This case should ideally not happen if KEYS is correctly initialized
            // and Claims are serializable. But we return an internal server error
            // if it does.
            return AuthError::TokenCreation.into_response();
        }
    };

    tracing::debug!("issuing token for user_id: {}", claims.user_id);
    let cookie = format!("auth_token={}; HttpOnly; Path=/; Max-Age={}", token, 60 * 60 * 24 * 7);

    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());

    (headers, Redirect::to("/")).into_response()
}


// ───── 4. Create_Jwt ────────────────────────

pub struct PartialClaims {
    pub email: String,
    pub user_id: Option<i64>,
    pub display_name: Option<String>,
    pub canvas_permissions: Option<HashMap<String, String>>,
}

impl Default for PartialClaims {
    fn default() -> Self {
        Self {
            email: String::new(),
            user_id: None,
            display_name: None,
            canvas_permissions: None,
        }
    }
}

const COOKIE_MAX_AGE_SECONDS: u64 = 60 * 60 * 24 * 7;


pub async fn create_cookie(pool: &SqlitePool, claims_data: PartialClaims) -> Result<String, AuthError> {
    let email = claims_data.email;
    let mut user_id = claims_data.user_id;
    let mut display_name = claims_data.display_name;
    let mut canvas_permissions = claims_data.canvas_permissions;

    // --- Step 1: Handle user_id and display_name ---
    if user_id.is_none() || display_name.is_none() {
        tracing::debug!("User ID or display name missing, fetching user details for email: {}", email);
        let user_row = sqlx::query!(
            "SELECT user_id, display_name FROM users WHERE email = ?",
            email
        )
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            tracing::error!("Database query error fetching user info: {:?}", e);
            AuthError::DbError
        })?
        .ok_or(AuthError::UserInfoNotFound)?;

        // FIX: The compiler error indicates `user_row.user_id` is already an `Option<i64>`.
        // We assign it directly without wrapping it in `Some()` again.
        if user_id.is_none() {
            user_id = user_row.user_id;
        }
        if display_name.is_none() {
            display_name = Some(user_row.display_name);
        }
    }

    let final_user_id = user_id.ok_or(AuthError::UserInfoNotFound)?;

    // --- Step 2: Handle canvas_permissions ---
    if canvas_permissions.is_none() {
        tracing::debug!("Canvas permissions missing, fetching permissions for user_id: {}", final_user_id);
        
        // This is where the macro syntax was incorrect.
        // `query_as!` expects a struct name. Since there is no struct for this,
        // we can use a temporary struct, or better yet, use `sqlx::query!` and then
        // manually collect the results into the HashMap.
        let user_permissions = sqlx::query!(
            "SELECT canvas_id, permission_level FROM Canvas_Permissions WHERE user_id = ?",
            final_user_id
        )
        .fetch_all(pool)
        .await
        .map_err(|e| {
            tracing::error!("Database query error fetching canvas permissions: {:?}", e);
            AuthError::DbError
        })?;
        
        // The result of `query!` is a Vec of structs with `canvas_id` and `permission_level` fields.
        // We can iterate over this to build the HashMap.
        canvas_permissions = Some(user_permissions.into_iter()
            .map(|row| (row.canvas_id, row.permission_level))
            .collect());
    }

    // --- Step 3: Finalize and Encode Claims ---
    let final_display_name = display_name.ok_or(AuthError::UserInfoNotFound)?;
    let final_canvas_permissions = canvas_permissions.ok_or(AuthError::UserInfoNotFound)?;

    // Use the new constant to set the expiration
    let exp = jsonwebtoken::get_current_timestamp() + COOKIE_MAX_AGE_SECONDS;

    let claims_to_encode = Claims {
        user_id: final_user_id,
        email,
        display_name: final_display_name,
        exp: exp as usize, // Cast to usize for the claims struct
        canvas_permissions: final_canvas_permissions,
    };

    let token = jsonwebtoken::encode(&jsonwebtoken::Header::default(), &claims_to_encode, &KEYS.encoding)
        .map_err(|e| {
            tracing::error!("Failed to create token in create_cookie: {:?}", e);
            AuthError::TokenCreation
        })?;

    tracing::debug!("Issuing cookie for user_id: {}", final_user_id);
    let cookie = format!("auth_token={}; HttpOnly; Path=/; Max-Age={}", token, COOKIE_MAX_AGE_SECONDS);

    Ok(cookie)
}
