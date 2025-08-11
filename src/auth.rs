use std::{collections::HashMap};

// src/auth.rs
use axum::{
    body::Body, extract::{FromRequestParts, State }, http::{header::{self, COOKIE}, request::Parts, HeaderMap, HeaderValue, Request, StatusCode}, middleware::Next, response::{IntoResponse, Redirect, Response}, Json
};
use jsonwebtoken::{decode, DecodingKey, EncodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2, PasswordHash, PasswordVerifier,
};
use sqlx::SqlitePool;

// Import the LazyLock from the main crate
use crate::{AppState, KEYS};

// ───── 1. Types and their impls ────────────
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub user_id: i64,
    pub email: String,
    pub display_name: String,

    /// Hard expiry: absolute epoch seconds
    pub exp: usize,

    /// Soft reissue time: absolute epoch seconds
    pub reissue_time: usize,

    pub canvas_permissions: HashMap<String, String>,
}

impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = Redirect;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {

        if let Some(claims) = parts.extensions.get::<Claims>() {
            tracing::debug!("Claims found in extensions, skipping decode");
            return Ok(claims.clone());
        }

        let cookies = parts.headers.get(COOKIE)
            .and_then(|hdr| hdr.to_str().ok())
            .unwrap_or("");
        tracing::debug!("Cookie header on request in from_request_parts: {:?}", cookies);

        let token = cookies
            .split(';')
            .map(|c| c.trim())
            .find_map(|cookie| {
                if cookie.starts_with("auth_token=") {
                    Some(cookie.trim_start_matches("auth_token=").to_string())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                tracing::debug!("No auth_token cookie found, redirecting to /login");
                Redirect::to("/login")
            })?;

        let token_data = decode::<Claims>(
            &token,
            &KEYS.decoding,
            &Validation::default(),
        ).map_err(|_| {
            tracing::debug!("Failed to decode JWT, redirecting to /login");
            Redirect::to("/login")
        })?;

        Ok(token_data.claims)
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
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let pool = state.pool.clone();
    let refresh_list = state.permission_refresh_list.clone();

    // Split request into parts and body
    let (mut parts, body) = req.into_parts();

    // Extract claims from mutable parts
    let claims_result = Claims::from_request_parts(&mut parts, &pool).await;
    let mut req = Request::from_parts(parts, body);

    let claims_result: Result<Claims, Redirect> = match claims_result {
        Ok(claims) => Ok(claims),
        Err(_) => Err(Redirect::to("/login")),
    };

    let now = jsonwebtoken::get_current_timestamp() as usize;
    let mut set_cookie_header: Option<HeaderMap> = None;
    tracing::debug!("\n\n---new request---");

    match claims_result {
        Ok(mut claims) => {
            // Hard expiration check
            if claims.exp <= now {
                tracing::debug!(
                    "Token for user_id={} expired at {}. URI: {:?}. Redirecting to /login.",
                    claims.user_id,
                    claims.exp,
                    req.uri()
                );
                return Redirect::to("/login").into_response();
            }

            // Check both soft-expire and refresh list
            let soft_expired = claims.reissue_time <= now;
            let refresh_list_entry = refresh_list.should_refresh(claims.user_id).await;

            if soft_expired || refresh_list_entry {
                tracing::debug!(
                    "Token for user_id={} needs refresh. soft_expired={}, refresh_list_entry={}, reissue_time={}, URI: {:?}",
                    claims.user_id,
                    soft_expired,
                    refresh_list_entry,
                    claims.reissue_time,
                    req.uri()
                );

                // Refresh claims from DB
                let partial_claims = PartialClaims {
                    email: claims.email.clone(),
                    user_id: Some(claims.user_id),
                    display_name: Some(claims.display_name.clone()),
                    canvas_permissions: None,
                    exp: claims.exp,
                };

                match get_claims(&pool, partial_claims).await {
                    Ok(fresh_claims) => {
                        claims = fresh_claims;

                        if let Ok(cookie_str) = get_cookie_from_claims(claims.clone()).await {
                            set_cookie_header = Some(create_cookie_header(cookie_str));
                        } else {
                            tracing::error!(
                                "Failed to create refreshed cookie for user_id={}",
                                claims.user_id
                            );
                            return Redirect::to("/login").into_response();
                        }

                        tracing::debug!(
                            "Issued refreshed token for user_id={} (new reissue_time={}).",
                            claims.user_id,
                            claims.reissue_time
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Could not refresh claims from DB for user_id={}: {:?}. Redirecting to /login.",
                            claims.user_id,
                            e
                        );
                        return Redirect::to("/login").into_response();
                    }
                }
            }

            tracing::debug!(
                "Authenticated user claims: user_id={}, email={}, display_name={}, exp={}, reissue_time={}, canvas_permissions={:?}. URI: {:?}",
                claims.user_id,
                claims.email,
                claims.display_name,
                claims.exp,
                claims.reissue_time,
                claims.canvas_permissions,
                req.uri()
            );

            req.extensions_mut().insert(claims);

            tracing::debug!("running handler now");
            let mut response = next.run(req).await;
            tracing::debug!("running handler done");

            // Add refreshed cookie if needed
            if let Some(cookie_headers) = set_cookie_header {
                if !response.headers().contains_key(axum::http::header::SET_COOKIE) {
                    tracing::debug!("response does not yet contain a cookie");
                    for (name, value) in cookie_headers.iter() {
                        response.headers_mut().insert(name, value.clone());
                    }
                } else {
                    tracing::debug!("response already contains a cookie");
                }
            }

            response
        }
        Err(redirect) => {
            tracing::debug!(
                "Unauthenticated request for {:?}, redirecting to /login",
                req.uri()
            );
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


pub async fn authorize_user(
    pool: &SqlitePool,
    email: &str,
    password: &str,
) -> Result<String, AuthError> {
    if email.is_empty() || password.is_empty() {
        return Err(AuthError::MissingCredentials);
    }

    // Fetch user_id and password_hash for authentication only
    let user_row = sqlx::query!(
        "SELECT user_id, password_hash FROM users WHERE email = ?",
        email
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::error!("Database query error during authorization (user fetch): {:?}", e);
        AuthError::DbError
    })?
    .ok_or(AuthError::WrongCredentials)?;

    // Verify password
    if verify_password(password, &user_row.password_hash).map_err(|_| AuthError::WrongCredentials)? {
        // Step 1: Get full claims
        let partial_claims = PartialClaims {
            email: email.to_string(),
            user_id: user_row.user_id,
            ..PartialClaims::default()
        };

        let claims = get_claims(pool, partial_claims).await?;

        // Step 2: Create cookie string from claims
        let cookie = get_cookie_from_claims(claims).await?;

        Ok(cookie)
    } else {
        tracing::info!("Authorization failed: Wrong password for user {}", email);
        Err(AuthError::WrongCredentials)
    }
}


// NEW: Builds a HeaderMap with the Set-Cookie header from a given cookie string.
pub fn create_cookie_header(cookie: String) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());
    headers
}


// ───── 4. Create_Jwt ────────────────────────

pub const EXPIRED_AFTER_SECONDS: usize = 60 * 60 * 24 * 7;
pub const REISSUE_AFTER_SECONDS: usize = 5 * 60;

pub struct PartialClaims {
    pub email: String,
    pub user_id: Option<i64>,
    pub display_name: Option<String>,
    pub canvas_permissions: Option<HashMap<String, String>>,
    pub exp: usize,
}

impl Default for PartialClaims {
    fn default() -> Self {
        Self {
            email: String::new(),
            user_id: None,
            display_name: None,
            canvas_permissions: None,
            exp: (jsonwebtoken::get_current_timestamp() as usize) + EXPIRED_AFTER_SECONDS,
        }
    }
}

pub async fn get_claims(
    pool: &SqlitePool,
    claims_data: PartialClaims,
) -> Result<Claims, AuthError> {
    let email = claims_data.email;
    let mut user_id = claims_data.user_id;
    let mut display_name = claims_data.display_name;
    let mut canvas_permissions = claims_data.canvas_permissions;

    // --- Step 1: Handle user_id and display_name ---
    if user_id.is_none() || display_name.is_none() {
        tracing::debug!(
            "User ID or display name missing, fetching user details for email: {}",
            email
        );
        let user_row = sqlx::query!(
            "SELECT user_id, display_name FROM users WHERE email = ?",
            email
        )
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            tracing::error!(
                "Database query error fetching user info: {:?}",
                e
            );
            AuthError::DbError
        })?
        .ok_or(AuthError::UserInfoNotFound)?;

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
        tracing::debug!(
            "Fetching Canvas permissions for user_id: {}",
            final_user_id
        );

        let user_permissions = sqlx::query!(
            "SELECT canvas_id, permission_level 
             FROM Canvas_Permissions 
             WHERE user_id = ?",
            final_user_id
        )
        .fetch_all(pool)
        .await
        .map_err(|e| {
            tracing::error!(
                "Database query error fetching canvas permissions: {:?}",
                e
            );
            AuthError::DbError
        })?;

        canvas_permissions = Some(
            user_permissions
                .into_iter()
                .map(|row| (row.canvas_id, row.permission_level))
                .collect(),
        );
    }

    // --- Step 3: Finalize Claims ---
    let final_display_name = display_name.ok_or(AuthError::UserInfoNotFound)?;
    let final_canvas_permissions = canvas_permissions.ok_or(AuthError::UserInfoNotFound)?;

    let now = jsonwebtoken::get_current_timestamp() as usize;

    Ok(Claims {
        user_id: final_user_id,
        email,
        display_name: final_display_name,
        exp: claims_data.exp, // keep from original PartialClaims
        reissue_time: now + REISSUE_AFTER_SECONDS,
        canvas_permissions: final_canvas_permissions,
    })
}

pub async fn get_cookie_from_claims(claims: Claims) -> Result<String, AuthError> {
    let token = jsonwebtoken::encode(&jsonwebtoken::Header::default(), &claims, &KEYS.encoding)
        .map_err(|e| {
            tracing::error!("Failed to create token in get_cookie_from_claims: {:?}", e);
            AuthError::TokenCreation
        })?;

    tracing::debug!(
        "Issuing cookie with claims: user_id={}, email={}, display_name={}, exp={}, canvas_permissions={:?}",
        claims.user_id,
        claims.email,
        claims.display_name,
        claims.exp,
        claims.canvas_permissions
    );
    tracing::debug!("    JWT={}\n", token);

    let cookie = format!(
        "auth_token={}; HttpOnly; Path=/; Max-Age={}; SameSite=Strict",
        token,
        EXPIRED_AFTER_SECONDS 
    );

    Ok(cookie)
}





// -------------- start of the update hash map stuff ------------------------

// As far as I can tell, there is no way to implement timely permission updates in users' JWTs without accessing server-side state on each user request.
// It is possible to do so without server state only if JWTs expire after a fixed interval.
// However, this approach either causes permission updates to take minutes to propagate 
// or requires frequently reissuing JWTs because of a short expiry time.
//
// Note that pushing updates through WebSockets alone is insufficient,
// because a user might not be connected to the web app but still possess a valid token.
//
// I believe I have found a good hybrid solution:
// Whenever changes are made to a user's permissions, an entry is added to a server-side hash map.
// When that user makes a request, the map is checked for an entry corresponding to the user.
// If such an entry exists, the user's JWT is refreshed before handling the request.
//
// To prevent the hash map from growing uncontrollably over time,
// JWTs have a reissue time of 5 minutes.
// If a request arrives with a JWT older than the reissue time, the server issues a new token valid for another 5 minutes.
// This means we can safely prune all entries from the hash map older than 5 minutes.
//
// Access to this server state is efficient (constant time complexity) and fast because it is stored in memory.
// Space complexity remains bounded due to the automatic pruning mechanism.

use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};
use std::time::{SystemTime, UNIX_EPOCH};

type UserId = i64;

#[derive(Clone)]
pub struct PermissionRefreshList {
    inner: Arc<RwLock<HashMap<UserId, usize>>>,
}

impl PermissionRefreshList {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn mark_user_for_refresh(&self, user_id: UserId) {
        let now = current_timestamp();
        let mut map = self.inner.write().await;
        map.insert(user_id, now);
    }

    pub async fn should_refresh(&self, user_id: UserId) -> bool {
        let mut map = self.inner.write().await;
        if map.remove(&user_id).is_some() {
            true
        } else {
            false
        }
    }

    pub async fn prune_old_entries(&self, max_age: usize) {
        let now = current_timestamp();
        let mut map = self.inner.write().await;
        map.retain(|_, &mut timestamp| now < timestamp + max_age);
    }
}

fn current_timestamp() -> usize {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize
}

pub async fn start_cleanup_task(refresh_list: Arc<PermissionRefreshList>) {
    let reissue_time: usize = REISSUE_AFTER_SECONDS;
    let prune_age = reissue_time * 2;
    let interval = Duration::from_secs(reissue_time as u64);

    loop {
        sleep(interval).await;
        tracing::debug!("running refresh List prune");
        refresh_list.prune_old_entries(prune_age).await;
        tracing::debug!("done with refresh List prune");
    }
}

