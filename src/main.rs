//! Parts of this code have been adapted from https://github.com/tokio-rs/axum/blob/main/examples/jwt/src/main.rs
//! ChatGPT and Google Gemini where used (interestingly Gemini is significantly better at rust than ChatGPT)
//! Example JWT authorization/authentication.
//!
//! Run with
//!
//! ```not_rust
//! JWT_SECRET=secret cargo run -p example-jwt
//! ```


// Quick instructions
//
// - get an authorization token:
//
// curl -s -w '\n' -H 'Content-Type: application/json' -d '{"client_id":"foo","client_secret":"bar"}' http://localhost:3000/authorize 
//
// - visit the protected area using the authorized token
//
// curl -s -w '\n' -H 'Content-Type: application/json' -H 'Authorization: Bearer eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJiQGIuY29tIiwiY29tcGFueSI6IkFDTUUiLCJleHAiOjEwMDAwMDAwMDAwfQ.M3LAZmrzUkXDC1q5mSzFAs_kJrwuKz3jOoDmjJ0G4gM' http://localhost:3000/protected
// curl -s -w '\n' -H 'Content-Type: application/json' -H 'Authorization: Bearer eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJiQGIuY29tIiwiY29tcGFueSI6IkFDTUUiLCJleHAiOjIwMDAwMDAwMDB9.KhqUwuS0eDsS3kU69CQWxHujLYfGuXljFDkuVmYAVTQ' http://localhost:3000/protected
//
// - try to visit the protected area using an invalid token
//
// curl -s -w '\n' -H 'Content-Type: application/json' -H 'Authorization: Bearer blahblahblah' http://localhost:3000/protected

//TODOs
// - event text field
// - integrate type script src files into the project
// - docker 


use axum::{
   extract::{FromRequestParts}, http::{header, request::Parts, HeaderMap, HeaderValue, StatusCode}, middleware::{self, Next}, response::{Html, IntoResponse, Redirect, Response}, routing::{any, get, post}, Form, Json, Router
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::{services::ServeDir};
use std::{env, fmt::Display, net::SocketAddr};
use std::sync::LazyLock;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use std::fs;
use std::collections::HashMap;
use tokio::sync::RwLock;



// ───── 1. Constants / statics ──────────────
static KEYS: LazyLock<Keys> = LazyLock::new(|| {
    let secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    Keys::new(secret.as_bytes())
});

// DB placeholder 
static USERS: LazyLock<RwLock<HashMap<String, String>>> = LazyLock::new(|| {
    RwLock::new(HashMap::new())
});

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
        .route("/logout", post(logout));


    // Determine the binding address based on an environment variable
    let host = env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("SERVER_PORT").unwrap_or_else(|_| "3000".to_string());

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


async fn login(Form(payload): Form<AuthPayload>) -> impl IntoResponse {
    match authorize_user(&payload).await {
        Ok(token) => jwt_response(token),
        Err(err) => err.into_response(),
    }
}

async fn register(Form(payload): Form<AuthPayload>) -> impl IntoResponse {
    if payload.email.is_empty() || payload.password.is_empty() {
        return AuthError::MissingCredentials.into_response();
    }

    {
        let mut users = USERS.write().await;

        if users.contains_key(&payload.email) {
            return AuthError::UserExists.into_response();
        }

        users.insert(payload.email.clone(), payload.password.clone());
    }

    match authorize_user(&payload).await {
        Ok(token) => jwt_response(token),
        Err(err) => err.into_response(),
    }
}


// Core authorization logic (shared by login + register)
async fn authorize_user(payload: &AuthPayload) -> Result<String, AuthError> {
    if payload.email.is_empty() || payload.password.is_empty() {
        return Err(AuthError::MissingCredentials);
    }

    let users = USERS.read().await;
    match users.get(&payload.email) {
        Some(stored_password) if stored_password == &payload.password => {
            let claims = Claims {
                email: payload.email.clone(),
                exp: 2_000_000_000,
            };

            let token = encode(&Header::default(), &claims, &KEYS.encoding)
                .map_err(|_| AuthError::TokenCreation)?;

            // Log user and token
            tracing::info!("Authorized user: {}, JWT: {}", payload.email, token);

            Ok(token)
        }
        _ => Err(AuthError::WrongCredentials),
    }
}

// Shared cookie + redirect response builder
fn jwt_response(token: String) -> Response {
    let cookie = format!("auth_token={}; HttpOnly; Path=/", token);

    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());

    (headers, Redirect::to("/")).into_response()
}

async fn login_page() -> impl IntoResponse {
    match fs::read_to_string("login.html") {
        Ok(contents) => Html(contents).into_response(),
        Err(_) => Html("<h1>Login page not found</h1>").into_response(),
    }
}

async fn register_page() -> impl IntoResponse {
    match fs:: read_to_string("register.html") {
        Ok(contents) => Html(contents).into_response(),
        Err(_) => Html("<h1>Register page not found</h1>").into_response(),

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
    UserExists,           // new error variant
    TokenCreation,
}


impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AuthError::WrongCredentials => (StatusCode::UNAUTHORIZED, "Wrong credentials"),
            AuthError::MissingCredentials => (StatusCode::BAD_REQUEST, "Missing credentials"),
            AuthError::UserExists => (StatusCode::CONFLICT, "User already exists"), 
            AuthError::TokenCreation => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Token creation error")
            }
        };
        let body = Json(json!({ "error": error_message }));
        (status, body).into_response()
    }
}

