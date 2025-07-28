//! Parts of this code have been adapted from https://github.com/tokio-rs/axum/blob/main/examples/jwt/src/main.rs
//! ChatGPT and Google Gemini where used (interestingly Gemini is significantly better at rust than ChatGPT)
//! Example JWT authorization/authentication.
//!
//! Run with
//!
//! ```not_rust
//! JWT_SECRET=secret cargo run -p example-jwt
//! ```

use axum::{
   extract::{FromRequestParts, State}, http::{header, request::Parts, HeaderMap, HeaderValue, StatusCode}, middleware::{self, Next}, response::{Html, IntoResponse, Redirect, Response}, routing::{any, get, post}, Form, Json, Router
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::{services::ServeDir};
use std::{env, net::SocketAddr};
use std::sync::LazyLock;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use std::fs;

// SQLX imports
use sqlx::sqlite::{SqlitePool, SqliteRow}; // Specific pool for SQLite
use sqlx::{Error as SqlxError, Row, query}; // Common sqlx traits/macros
use sqlx::migrate::Migrator; // For database migrations
use dotenvy::dotenv; // For loading .env files

// Password hashing imports
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2, PasswordHash, PasswordVerifier,
};


// ───── 1. Constants / statics ──────────────
static KEYS: LazyLock<Keys> = LazyLock::new(|| {
    let secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    Keys::new(secret.as_bytes())
});

// Static Migrator instance (ensure your `migrations` directory exists at project root)
static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

// ───── 2. Main entrypoint ──────────────────
#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}=debug", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // --- data base ---

    // Load .env file for local development (won't affect Docker, which uses env vars directly)
    dotenv().ok();
    tracing::info!("Environment variables loaded."); // Add this
    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in .env or environment variables");
    tracing::info!("DATABASE_URL: {}", database_url); // Add this to see what path is being used

    if database_url.starts_with("sqlite://") {
        let db_path_str = database_url.trim_start_matches("sqlite://");
        let db_path = std::path::Path::new(db_path_str);
        if let Some(parent_dir) = db_path.parent() {
            if !parent_dir.exists() {
                tracing::info!("Creating database directory: {:?}", parent_dir);
                std::fs::create_dir_all(parent_dir)
                    .expect("Failed to create database directory.");
            }
        }
    }

    tracing::info!("Connecting to database at: {}", database_url);
    let pool = SqlitePool::connect(&database_url)
        .await
        .expect("Failed to create SQLite pool. Check DATABASE_URL and database file permissions.");


    tracing::info!("Running database migrations...");
    MIGRATOR.run(&pool).await.expect("Failed to run database migrations.");
    tracing::info!("Database migrations applied successfully.");

    // --- routing ---

    // The protected service: ServeDir with its own 404 handler
    let protected_static_files_service = ServeDir::new("./public")
        .not_found_service(any(handle_404)); // This service correctly handles GET/HEAD for files and 404s


    let app = Router::new()
        .route("/api/user-info", get(get_user_info)) // Add this line
        // Define routes that require authentication before the middleware
        // This makes sure these specific routes are protected.
        .route("/profile", post(update_profile))
        // The `fallback_service` now takes the protected static files directly.
        .fallback_service(protected_static_files_service)
        // Apply the authentication middleware to ALL routes *above* this point
        // that are not explicitly defined above. This includes the fallback.
        .layer(middleware::from_fn(auth_middleware))

        .route("/login", get(login_page))
        .route("/login", post(login))
        .route("/register", get(register_page))
        .route("/register", post(register))
        .route("/logout", post(logout))
        .with_state(pool.clone());


    // --- network stuff --- 

    // Determine the binding address based on an environment variable
    let host = env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("SERVER_PORT").unwrap_or_else(|_| "8080".to_string());

    let addr_str = format!("{}:{}", host, port);
    let addr: SocketAddr = addr_str.parse().expect("Invalid SERVER_HOST:SERVER_PORT provided");

    // Use the dynamically determined address for binding
    let listener = tokio::net::TcpListener::bind(addr) // Use the 'addr' variable here
        .await
        .unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}



async fn auth_middleware(
    claims_result: Result<Claims, Redirect>,
    request: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    match claims_result {
        Ok(claims) => {
            tracing::debug!("Authenticated user: {} (ID: {}) accessing {:?}", claims.email, claims.user_id, request.uri());
            // --- ADD THIS LINE ---
            let mut request = request;
            request.extensions_mut().insert(claims); // Store the Claims in extensions
            // --- END ADDITION ---
            next.run(request).await
        }
        Err(redirect) => {
            tracing::debug!("Unauthenticated request for {:?}, redirecting to /login", request.uri());
            redirect.into_response()
        }
    }
}

// Custom handler for 404 errors
async fn handle_404() -> Response {
    (StatusCode::NOT_FOUND, "404 Not Found").into_response()
}

// ───── 3. Handlers ─────────────────────────

async fn get_user_info(
    claims: Claims, // The Claims extractor will get this from the request extensions
) -> impl IntoResponse {
    // You already have the claims thanks to the auth_middleware and FromRequestParts for Claims!
    Json(json!({
        "user_id": claims.user_id,
        "email": claims.email,
        "display_name": claims.display_name,
    }))
}

// Update User Profile
async fn update_profile(
    State(pool): State<SqlitePool>,
    claims: Claims, // Extracted by auth_middleware and FromRequestParts
    Form(payload): Form<UpdateUserPayload>, // New payload for updates
) -> impl IntoResponse {
    // Check if there's anything to update
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

    // Handle email update if provided
    if let Some(new_email) = payload.email {
        if new_email.is_empty() {
             tx.rollback().await.ok();
             return (StatusCode::BAD_REQUEST, Json(json!({"error": "Email cannot be empty."}))).into_response();
        }
        // Check if the new email already exists for another user
        match query!("SELECT user_id FROM users WHERE email = ? AND user_id != ?", new_email, claims.user_id)
            .fetch_optional(&mut *tx)
            .await
        {
            Ok(Some(_)) => {
                tx.rollback().await.ok();
                tracing::warn!("Profile update failed: Email '{}' already taken by another user.", new_email);
                return AuthError::UserExists.into_response(); // Email already taken
            }
            Ok(None) => {
                // Email is unique, proceed with update
                match query!("UPDATE users SET email = ? WHERE user_id = ?", new_email, claims.user_id)
                    .execute(&mut *tx)
                    .await
                {
                    Ok(_) => {
                        tracing::info!("User {} (ID: {}) updated email to '{}'.", claims.email, claims.user_id, new_email);
                        updated_email = new_email; // Update for new JWT
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

    // Handle display_name update if provided
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
                updated_display_name = new_display_name; // Update for new JWT
            }
            Err(e) => {
                tx.rollback().await.ok();
                tracing::error!("Failed to update display name for user {}: {:?}", claims.user_id, e);
                return AuthError::DbError.into_response();
            }
        }
    }

    // Commit the transaction
    match tx.commit().await {
        Ok(_) => tracing::debug!("Transaction committed for user {}", claims.user_id),
        Err(e) => {
            tracing::error!("Failed to commit transaction for user {}: {:?}", claims.user_id, e);
            return AuthError::DbError.into_response();
        }
    }

    // Issue a new JWT with updated claims
    let new_claims = Claims {
        user_id: claims.user_id,
        email: updated_email,
        display_name: updated_display_name,
        exp: 2_000_000_000, // Re-use the expiration, or calculate new based on current time
    };

    let new_token = match encode(&Header::default(), &new_claims, &KEYS.encoding) {
        Ok(token) => token,
        Err(e) => {
            tracing::error!("Failed to create new token after profile update: {:?}", e);
            return AuthError::TokenCreation.into_response();
        }
    };

    // Return the new JWT as a cookie
    jwt_response(new_token)
}




async fn logout() -> impl IntoResponse {
    let mut headers = HeaderMap::new();

    // Set auth_token cookie with an expired date
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_static("auth_token=; HttpOnly; Path=/; Max-Age=0"),
    );

    (headers, Redirect::to("/login")).into_response()
}


async fn login(
    State(pool): State<SqlitePool>, // Get the database pool from state
    Form(payload): Form<LoginPayload> // <-- CHANGED: Use LoginPayload here
) -> impl IntoResponse {
    match authorize_user(&pool, &payload).await { // Pass pool to authorize_user
        Ok(token) => jwt_response(token),
        Err(err) => err.into_response(),
    }
}

async fn register(
    State(pool): State<SqlitePool>, // Get the database pool from state
    Form(payload): Form<AuthPayload>
) -> impl IntoResponse {

    // Ensure all required fields are present, including display_name
    if payload.email.is_empty() || payload.password.is_empty() || payload.display_name.is_empty() {
        return AuthError::MissingCredentials.into_response();
    }

    // Hash the password
    let password_hash = match hash_password(&payload.password) {
        Ok(hash) => hash,
        Err(_) => return AuthError::PasswordHashingFailed.into_response(), // <--- Handle error explicitly
    };

    // Insert user into the database, including display_name
    match query!(
        "INSERT INTO users (email, password_hash, display_name) VALUES (?, ?, ?)",
        payload.email,
        password_hash,
        payload.display_name // <-- Add display_name here
    )
    .execute(&pool)
    .await
    {
        Ok(_) => {
            tracing::info!("User {} registered successfully.", payload.email);
            // After successful registration, attempt to authorize (log in) the user
            match authorize_user(&pool, &payload).await { // Pass pool to authorize_user
                Ok(token) => jwt_response(token),
                Err(err) => err.into_response(),
            }
        }
        Err(SqlxError::Database(db_error)) if db_error.code() == Some("2067".into()) => { // SQLITE_CONSTRAINT_UNIQUE
            tracing::warn!("Registration failed: User {} already exists.", payload.email);
            AuthError::UserExists.into_response()
        }
        Err(e) => {
            tracing::error!("Failed to register user {}: {:?}", payload.email, e);
            AuthError::DbError.into_response() // New generic DB error variant
        }
    }
}


// Core authorization logic (shared by login + register)
// This function needs to be generic enough to accept both AuthPayload (from register)
// and LoginPayload (from login). We'll achieve this by making it generic over `T`
// and adding a trait bound.
async fn authorize_user<T>(
    pool: &SqlitePool,
    payload: &T // <-- CHANGED: Generic over T
) -> Result<String, AuthError>
where
    T: AuthCommon + Send + Sync + 'static, // <-- NEW: Trait bound for common fields
{
    if payload.email().is_empty() || payload.password().is_empty() { // <-- CHANGED: Use methods
        return Err(AuthError::MissingCredentials);
    }

    // Query the database for the user, selecting user_id and display_name
    let user_row: Option<SqliteRow> = query(
        "SELECT user_id, email, password_hash, display_name FROM users WHERE email = ?"
    )
    .bind(payload.email()) // <-- CHANGED: Use method
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::error!("Database query error during authorization: {:?}", e);
        AuthError::DbError
    })?;

    match user_row {
        Some(row) => {
            let stored_password_hash: String = row.try_get("password_hash")
                .map_err(|e| {
                    tracing::error!("Failed to get password_hash from row: {:?}", e);
                    AuthError::DbError
                })?;

            // Verify the password
            if verify_password(payload.password(), &stored_password_hash).map_err(|_| AuthError::WrongCredentials)? { // <-- CHANGED: Use method
                // Extract user_id and display_name for JWT claims
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

                let claims = Claims {
                    user_id,
                    email: payload.email().to_string(), // <-- CHANGED: Use method and clone
                    display_name,
                    exp: 2_000_000_000, // JWT expiration timestamp
                };

                let token = encode(&Header::default(), &claims, &KEYS.encoding)
                    .map_err(|_| AuthError::TokenCreation)?;

                tracing::info!("Authorized user: {} (ID: {})", claims.email, claims.user_id);
                Ok(token)
            } else {
                tracing::warn!("Authorization failed: Wrong password for user {}", payload.email()); // <-- CHANGED: Use method
                Err(AuthError::WrongCredentials)
            }
        }
        None => {
            tracing::warn!("Authorization failed: User {} not found.", payload.email()); // <-- CHANGED: Use method
            Err(AuthError::WrongCredentials) // User not found
        }
    }
}


// Password Hashing Helper
fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2.hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
}

// Password Verification Helper
fn verify_password(password: &str, hashed_password: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed_hash = PasswordHash::new(hashed_password)?;
    Ok(Argon2::default().verify_password(password.as_bytes(), &parsed_hash).is_ok())
}


// Shared cookie + redirect response builder
fn jwt_response(token: String) -> Response {
    let cookie = format!("auth_token={}; HttpOnly; Path=/; Max-Age={}", token, 60 * 60 * 24 * 7);

    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());

    (headers, Redirect::to("/")).into_response()
}

async fn login_page() -> impl IntoResponse {
    match fs::read_to_string("login.html") {
        Ok(contents) => Html(contents).into_response(),
        Err(_) => {
            tracing::error!("login.html not found!");
            Html("<h1>Login page not found</h1>").into_response()
        },
    }
}

async fn register_page() -> impl IntoResponse {
    match fs:: read_to_string("register.html") {
        Ok(contents) => Html(contents).into_response(),
        Err(_) => {
            tracing::error!("register.html not found!");
            Html("<h1>Register page not found</h1>").into_response()
        },
    }
}

// ───── 4. Types and their impls ────────────
#[derive(Debug, Serialize, Deserialize, Clone)]
struct Claims {
    user_id: i64,
    email: String,
    display_name: String,
    exp: usize,
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

        // Find the `auth_token` cookie manually
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


struct Keys {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl Keys {
    fn new(secret: &[u8]) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
        }
    }
}

#[derive(Debug, Deserialize)]
struct AuthPayload {
    email: String,
    password: String,
    display_name: String,
}


// LoginPayload for Login (only needs email and password)
#[derive(Debug, Deserialize)]
struct LoginPayload {
    email: String,
    password: String,
}

// For user profile updates (optional fields)
#[derive(Debug, Deserialize)]
struct UpdateUserPayload {
    email: Option<String>,
    display_name: Option<String>,
}

// Trait to abstract common fields for authorize_user
trait AuthCommon {
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
enum AuthError {
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