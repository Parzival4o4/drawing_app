// src/main.rs
use axum::{
    middleware,
    routing::{any, get, post},
    Router,
};
use sqlx::sqlite::SqlitePool;
use sqlx::migrate::Migrator;
use tower_http::services::ServeDir;
use std::{env, net::SocketAddr};
use std::sync::LazyLock; // Import LazyLock here
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use dotenvy::dotenv;

// Import modules
mod auth;
mod handlers;

// Re-export types from auth and handlers for main's use
use auth::auth_middleware; // Only need auth_middleware from auth
use handlers::{
    get_user_info, handle_404, login_page, login, logout, register_page, register, update_profile, create_canvas
};

// ───── 1. Constants / statics ──────────────
// Corrected LazyLock type annotation
pub(crate) static KEYS: LazyLock<auth::Keys> = LazyLock::new(|| {
    let secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    auth::Keys::new(secret.as_bytes())
});

// Static Migrator instance (ensure your `migrations` directory exists at project root)
static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

// ───── 2. Main entrypoint ──────────────────
#[tokio::main]
async fn main() {
    setup_tracing();

    let pool = setup_database().await;

    let app = create_app_router(pool);

    start_server(app).await;
}

// ───── 3. Helper Functions for Main ───────

fn setup_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}=debug", env!("CARGO_CRATE_NAME")).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    tracing::info!("Tracing initialized.");
}

async fn setup_database() -> SqlitePool {
    dotenv().ok();
    tracing::info!("Environment variables loaded.");
    let database_url = env::var("DATABASE_URL")
        .expect("JWT_SECRET must be set and DATABASE_URL must be set in .env or environment variables");
    tracing::info!("DATABASE_URL: {}", database_url);

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

    pool
}

fn create_app_router(pool: SqlitePool) -> Router {
    let protected_static_files_service = ServeDir::new("./public")
        .not_found_service(any(handle_404));

    Router::new()
        // Routes that need authentication, placed *before* the auth_middleware
        .route("/api/user-info", get(get_user_info))
        .route("/profile", post(update_profile))
        .route("/api/canvases", post(create_canvas))
        // Apply auth middleware to everything above this point, including the fallback.
        // The middleware will add Claims to the request extensions.
        .fallback_service(protected_static_files_service)
        .layer(middleware::from_fn(auth_middleware))

        // Routes that do NOT need authentication, placed *after* the auth_middleware layer
        .route("/login", get(login_page))
        .route("/login", post(login))
        .route("/register", get(register_page))
        .route("/register", post(register))
        .route("/logout", post(logout))
        .with_state(pool) // The pool is moved here, as Router takes ownership.
}

async fn start_server(app: Router) {
    let host = env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("SERVER_PORT").unwrap_or_else(|_| "8080".to_string());

    let addr_str = format!("{}:{}", host, port);
    let addr: SocketAddr = addr_str.parse().expect("Invalid SERVER_HOST:SERVER_PORT provided");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap();
    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}