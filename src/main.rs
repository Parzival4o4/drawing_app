// src/main.rs
use axum::{
    routing::{ get, post}, Router
};
use sqlx::sqlite::SqlitePool;
use sqlx::migrate::Migrator;
use tower_http::services::{ServeDir, ServeFile};
use std::{env, net::SocketAddr};
use std::sync::LazyLock; // Import LazyLock here
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use dotenvy::dotenv;

// Import modules
mod auth;
mod handlers;
mod websocket_handlers;
mod socket_claims_manager;
mod canvas_manager;
mod identifiable_web_socket;

// Re-export types from auth and handlers for main's use
use auth::{auth_middleware, PermissionRefreshList}; // Only need auth_middleware from auth
use handlers::{
    get_user_info, update_profile};
use std::sync::Arc;

use crate::{
    auth::start_cleanup_task, 
    handlers::{create_canvas, get_canvas_list, get_canvas_permissions, login, logout, register, update_canvas_permissions}, 
    websocket_handlers::{ws_handler},
    socket_claims_manager::{ SocketClaimsManager},
    canvas_manager::{CanvasManager}
};

// ───── 1. Constants / statics ──────────────
// Corrected LazyLock type annotation
pub(crate) static KEYS: LazyLock<auth::Keys> = LazyLock::new(|| {
    let secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    auth::Keys::new(secret.as_bytes())
});

// Static Migrator instance (ensure your `migrations` directory exists at project root)
static MIGRATOR: Migrator = sqlx::migrate!("./migrations");


#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub permission_refresh_list: Arc<PermissionRefreshList>,
    // pub active_connections: WebSocketConnections,
    pub canvas_manager: CanvasManager,
    pub socket_claims_manager: SocketClaimsManager,
}

// ───── Main entrypoint ──────────────────
#[tokio::main]
async fn main() {
    let _ = setup_tracing();
    let pool = setup_database().await;
    let permission_refresh_list = Arc::new(PermissionRefreshList::new());

    // Initialize the WebSocketConnections and CanvasManager structs
    let canvas_manager = CanvasManager::new();
    let socket_claims_manager = SocketClaimsManager::new();

    let app_state = AppState {
        pool: pool.clone(),
        permission_refresh_list: permission_refresh_list.clone(),
        canvas_manager: canvas_manager.clone(),
        socket_claims_manager: socket_claims_manager.clone()
    };

    tokio::spawn(start_cleanup_task(permission_refresh_list.clone()));

    let app = create_app_router(app_state);
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

fn create_app_router(state: AppState) -> Router {
    // This service handles requests for files in the "./public" directory.
    let spa_service = ServeDir::new("./public").not_found_service(
        ServeFile::new("./public/index.html")
    );

    // Protected API routes that require authentication.
    // We nest them under a `/api` path and apply the auth middleware.
    let protected_routes = Router::new()
        .route("/me", get(get_user_info))
        .route("/user/update", post(update_profile))
        .route("/canvases/create", post(create_canvas))
        .route("/canvases/list", get(get_canvas_list))
        .route("/canvas/{canvas_id}/permissions", post(update_canvas_permissions).get(get_canvas_permissions))
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth_middleware));

    // Public API routes for authentication and other unauthenticated endpoints.
    let public_api_routes = Router::new()
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/register", post(register));

    // Combine all routes and services into the final application router.
    Router::new()
        .nest("/api", public_api_routes.merge(protected_routes))
        .route("/ws", get(ws_handler))
        .fallback_service(spa_service)
        .with_state(state)
}




async fn start_server(app: Router) {
    let host = env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("SERVER_PORT").unwrap_or_else(|_| "8080".to_string());

    let addr_str = format!("{}:{}", host, port);
    let addr: SocketAddr = addr_str.parse().expect("Invalid SERVER_HOST:SERVER_PORT provided");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap();
    tracing::info!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}