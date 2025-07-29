// src/auth.rs
use axum::{
    extract::{FromRequestParts },
    http::{header, request::Parts, HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    Json,
};
use jsonwebtoken::{decode, DecodingKey, EncodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2, PasswordHash, PasswordVerifier,
};

// Import the LazyLock from the main crate
use crate::KEYS;

// ───── 1. Types and their impls ────────────
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub user_id: i64,
    pub email: String,
    pub display_name: String,
    pub exp: usize,
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

#[derive(Debug, Deserialize)]
pub struct AuthPayload {
    pub email: String,
    pub password: String,
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginPayload {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserPayload {
    pub email: Option<String>,
    pub display_name: Option<String>,
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

#[derive(Debug)]
pub enum AuthError {
    WrongCredentials,
    MissingCredentials,
    UserExists,
    TokenCreation,
    PasswordHashingFailed,
    DbError,
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
pub fn jwt_response(token: String) -> Response {
    tracing::debug!("issuing token: {}", token);
    let cookie = format!("auth_token={}; HttpOnly; Path=/; Max-Age={}", token, 60 * 60 * 24 * 7);

    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());

    (headers, Redirect::to("/")).into_response()
}