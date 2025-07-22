//! Parts of this code have been adapted from https://github.com/tokio-rs/axum/blob/main/examples/jwt/src/main.rs
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



use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode, header, HeaderMap, HeaderValue},
    response::{Html, IntoResponse, Response, Redirect},
    routing::{get, post},
    Form, Json, Router,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fmt::Display;
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

    let app = Router::new()
        .route("/", get(home))
        .route("/login", get(login_page))
        .route("/login", post(login))
        .route("/register", get(register_page))
        .route("/register", post(register)); 

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

// ───── 3. Handlers ─────────────────────────

async fn login(Form(payload): Form<AuthPayload>) -> impl IntoResponse {
    match authorize(Json(payload)).await {
        Ok(Json(body)) => {
            // Set the JWT as an HTTP-only cookie
            let cookie = format!(
                "auth_token={}; HttpOnly; Path=/",
                body.access_token
            );

            let mut headers = HeaderMap::new();
            headers.insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());

            // Redirect to home
            (headers, Redirect::to("/")).into_response()
        }
        Err(err) => err.into_response(),
    }
}



async fn register(Form(payload): Form<AuthPayload>) -> impl IntoResponse {
    if payload.email.is_empty() || payload.password.is_empty() {
        return AuthError::MissingCredentials.into_response();
    }

    // create the user in the "DB"
    {
        let mut users = USERS.write().await;

        if users.contains_key(&payload.email) {
            return AuthError::WrongCredentials.into_response(); // Or use a clearer error type
        }

        users.insert(payload.email.clone(), payload.password.clone());
    }

    // Call `authorize()` to generate JWT and redirect
    authorize(Json(payload)).await
        .map(|Json(body)| {
            let cookie = format!(
                "auth_token={}; HttpOnly; Path=/",
                body.access_token
            );
            let mut headers = HeaderMap::new();
            headers.insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());
            (headers, Redirect::to("/")).into_response()
        })
        .unwrap_or_else(|e| e.into_response())
}


async fn authorize(Json(payload): Json<AuthPayload>) -> Result<Json<AuthBody>, AuthError> {
    if payload.email.is_empty() || payload.password.is_empty() {
        return Err(AuthError::MissingCredentials);
    }

    let users = USERS.read().await;

    match users.get(&payload.email) {
        Some(stored_password) if stored_password == &payload.password => {
            let claims = Claims {
                sub: payload.email.clone(),
                company: "ACME".to_owned(),
                exp: 2000000000,
            };

            let token = encode(&Header::default(), &claims, &KEYS.encoding)
                .map_err(|_| AuthError::TokenCreation)?;

            Ok(Json(AuthBody::new(token)))
        }
        _ => Err(AuthError::WrongCredentials),
    }
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

async fn home(_claims: Claims) -> impl IntoResponse {
    match fs::read_to_string("home.html") {
        Ok(contents) => Html(contents).into_response(),
        Err(_) => Html("<h1>Home page not found</h1>").into_response(),
    }
}



// ───── 4. Types and their impls ────────────
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    company: String,
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
        write!(f, "Email: {}\nCompany: {}", self.sub, self.company)
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

#[derive(Debug, Serialize)]
struct AuthBody {
    access_token: String,
    token_type: String,
}

impl AuthBody {
    fn new(access_token: String) -> Self {
        Self {
            access_token,
            token_type: "Bearer".to_string(),
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
    TokenCreation,
    InvalidToken,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AuthError::WrongCredentials => (StatusCode::UNAUTHORIZED, "Wrong credentials"),
            AuthError::MissingCredentials => (StatusCode::BAD_REQUEST, "Missing credentials"),
            AuthError::TokenCreation => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Token creation error")
            }
            AuthError::InvalidToken => (StatusCode::BAD_REQUEST, "Invalid token"),
        };
        let body = Json(json!({ "error": error_message }));
        (status, body).into_response()
    }
}
