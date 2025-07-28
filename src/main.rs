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
use std::{env, fmt::Display, net::SocketAddr};
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
        // The `fallback_service` now takes the protected static files directly.
        .fallback_service(protected_static_files_service) // <--- Use the service directly here
        // Apply the authentication middleware to ALL routes *above* this point
        // that are not explicitly defined above. This includes the fallback.
        .layer(middleware::from_fn(auth_middleware)) // <--- Apply middleware here!

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



// This middleware runs *before* the inner service (ServeDir in this case)
async fn auth_middleware(
    claims_result: Result<Claims, Redirect>, // Attempt to extract Claims
    request: axum::http::Request<axum::body::Body>, // The incoming request
    next: Next, // The next service in the stack (ServeDir)
) -> Response {
    match claims_result {
        Ok(claims) => {
            // User is authenticated, proceed to the next service (ServeDir)
            tracing::debug!("Authenticated user: {:?} accessing {:?}", claims.email, request.uri());
            next.run(request).await
        }
        Err(redirect) => {
            // User is not authenticated, redirect to login page
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
    Form(payload): Form<AuthPayload>
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
    if payload.email.is_empty() || payload.password.is_empty() {
        return AuthError::MissingCredentials.into_response();
    }

    // Hash the password
    let password_hash = match hash_password(&payload.password) {
        Ok(hash) => hash,
        Err(_) => return AuthError::PasswordHashingFailed.into_response(), // <--- Handle error explicitly
    };

    // Insert user into the database
    match query!(
        "INSERT INTO users (email, password_hash) VALUES (?, ?)",
        payload.email,
        password_hash
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
async fn authorize_user(
    pool: &SqlitePool, // Accept the database pool
    payload: &AuthPayload
) -> Result<String, AuthError> {
    if payload.email.is_empty() || payload.password.is_empty() {
        return Err(AuthError::MissingCredentials);
    }

    // Query the database for the user
    let user_row: Option<SqliteRow> = query("SELECT id, email, password_hash FROM users WHERE email = ?")
        .bind(&payload.email)
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
            if verify_password(&payload.password, &stored_password_hash).map_err(|_| AuthError::WrongCredentials)? {
                let claims = Claims {
                    email: payload.email.clone(),
                    exp: 2_000_000_000, // JWT expiration timestamp
                };

                let token = encode(&Header::default(), &claims, &KEYS.encoding)
                    .map_err(|_| AuthError::TokenCreation)?;

                tracing::info!("Authorized user: {}", payload.email);
                Ok(token)
            } else {
                tracing::warn!("Authorization failed: Wrong password for user {}", payload.email);
                Err(AuthError::WrongCredentials)
            }
        }
        None => {
            tracing::warn!("Authorization failed: User {} not found.", payload.email);
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
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    email: String,
    exp: usize,
}


impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = Redirect;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
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


impl Display for Claims {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Email: {}", self.email)
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
}

#[derive(Debug)]
enum AuthError {
    WrongCredentials,
    MissingCredentials,
    UserExists,
    TokenCreation,
    PasswordHashingFailed, // New error variant for hashing issues
    DbError,               // Generic database error
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