// src/handlers.rs
// Add these imports to your handlers.rs
use axum::{extract::{ws::{Message, WebSocket}, State, WebSocketUpgrade}, response::IntoResponse};
use futures::StreamExt;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{mpsc,  RwLock};
use crate::auth::{get_claims, Claims, PartialClaims};
use crate::AppState; // Import AppState
use tokio::time::{interval, Duration};
use futures::SinkExt;



// A struct to hold the sender and the claims for each connection.
// This is the "wrapped connection" you were thinking of.
#[derive(Debug)]
pub struct WebSocketConnection {
    pub user_claims: Claims,
    // Using a channel sender to send messages from other parts of the application
    // to this specific connection. This is a robust alternative to storing SplitSink directly.
    pub message_sender: mpsc::Sender<Message>,
    // TODO add a list of subscibed canvases
}

// New struct to encapsulate the active connections and their management logic
#[derive(Clone)]
pub struct WebSocketConnections {
    inner: Arc<RwLock<HashMap<i64, WebSocketConnection>>>,
}

impl WebSocketConnections {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Updates the claims for an active WebSocket connection.
    /// Returns true if the user was found and updated, false otherwise.
    pub async fn update_user_claims(&self, user_id: i64, updated_claims: Claims) -> bool {
        let mut map = self.inner.write().await;
        if let Some(conn) = map.get_mut(&user_id) {
            conn.user_claims = updated_claims;
            tracing::info!("Updated claims for user {} in active connections.", user_id);
            true
            // TODO check if the premissions on subsicbed canvases need to be changed. 
        } else {
            tracing::debug!("User {} not found in active connections. No claims to update.", user_id);
            false
        }
    }
}


// The ws_handler is now the entry point for the router.
// It receives the WebSocketUpgrade and Claims from the Axum extractors.
// This is the correct signature for a router handler.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    mut claims: Claims,
    State(state): State<AppState>,
) -> impl IntoResponse {

    let now = jsonwebtoken::get_current_timestamp() as usize;

    // Check if the JWT is soft-expired or on the refresh list.
    // Use `should_refresh_no_remove` for this check to avoid removing the entry from the list
    // if a full HTTP request refresh hasn't occurred yet.
    let soft_expired = claims.reissue_time <= now;
    let refresh_list_entry = state.permission_refresh_list.has_pending_refresh(claims.user_id).await;
    
    // If the token is soft-expired or needs a refresh, get the latest claims from the DB.
    if soft_expired || refresh_list_entry {
        tracing::debug!(
            "WebSocket token for user {} needs refresh. soft_expired: {}, refresh_list_entry: {}",
            claims.user_id, soft_expired, refresh_list_entry
        );

        let partial_claims = PartialClaims{
            email: claims.email.clone(),
            user_id: Some(claims.user_id), 
            display_name: Some(claims.display_name.clone()),
            ..PartialClaims::default()
        };

        match get_claims(&state.pool, partial_claims).await {
            Ok(fresh_claims) => {
                claims = fresh_claims;
                tracing::debug!("Claims refreshed from DB for WebSocket connection.");
            }
            Err(e) => {
                tracing::warn!("Failed to refresh claims for WebSocket user {}: {:?}", claims.user_id, e);
                // Return an error response, preventing the WebSocket from being established
                // with outdated claims.
                return axum::response::Response::builder()
                    .status(axum::http::StatusCode::UNAUTHORIZED)
                    .body(axum::body::Body::empty())
                    .unwrap()
                    .into_response();
            }
        }
    }

    let user_id = claims.user_id;
    tracing::debug!("Upgrading WebSocket connection for user {}", user_id);

    // Use on_upgrade to handle the actual WebSocket connection.
    // We pass the claims and state to the new handler function.
    ws.on_upgrade(move |socket| handle_websocket(socket, claims, state))
}


// The actual logic for handling the WebSocket connection is now in this separate function.
// It receives a WebSocket (after the upgrade) and the Claims, as it needs.
async fn handle_websocket(socket: WebSocket, claims: Claims, state: AppState) {
    let user_id = claims.user_id;
    let (mut sender, mut receiver) = socket.split();

    // Create a channel for internal message passing to this connection.
    let (tx, mut rx) = mpsc::channel::<Message>(128);

    // Add the connection to the Active Connections map
    let wrapped_connection = WebSocketConnection {
        user_claims: claims,
        message_sender: tx,
    };
    state.active_connections.inner.write().await.insert(user_id, wrapped_connection);
    
    // A separate task to handle sending messages from the mpsc channel
    let mut sender_task = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if let Err(e) = sender.send(message).await {
                tracing::error!("Failed to send message to user {}: {}", user_id, e);
                break;
            }
        }
    });

    tracing::info!("User {} connected via WebSocket.", user_id);

    // Timer for periodic server-to-client messages (for testing)
    let mut periodic_timer = interval(Duration::from_secs(10));
    periodic_timer.tick().await;

    loop {
        tokio::select! {
            // Case 1: Handle incoming messages from the client.
            Some(Ok(message)) = receiver.next() => {
                match message {
                    Message::Text(text) => {
                        // Get the current claims from the map before logging
                        let active_connections_lock = state.active_connections.inner.read().await;
                        if let Some(conn) = active_connections_lock.get(&user_id) {
                            tracing::info!(
                                "Received message from user {}: {} with claims {:?}",
                                user_id, text, conn.user_claims
                            );
                        } else {
                            tracing::warn!("Received message from user not in active connections map: {}", user_id);
                        }
                    }
                    Message::Close(_) => {
                        tracing::info!("User {} sent a close frame. Exiting loop.", user_id);
                        break;
                    }
                    _ => {}
                }
            }
            
            // Case 2: Handle periodic messages from the server.
            _ = periodic_timer.tick() => {
                let message = Message::Text("hello client".to_string().into());
                let active_connections_lock = state.active_connections.inner.read().await;
                if let Some(conn) = active_connections_lock.get(&user_id) {
                    tracing::info!(
                        "Sending periodic message to user {} with claims {:?}",
                        user_id, conn.user_claims
                    );
                    if let Err(e) = conn.message_sender.send(message).await {
                        tracing::error!("Failed to send message via channel to user {}: {}. Exiting loop.", user_id, e);
                        break;
                    }
                } else {
                    tracing::warn!("Periodic message for user {} failed. User not in map.", user_id);
                }
            }
            
            // This ensures the loop exits if all streams are exhausted
            else => {
                break;
            }
        }
    }
    
    // Remove the connection from the map when the loop exits.
    tracing::info!("User {}'s WebSocket connection closed. Removing from map.", user_id);
    state.active_connections.inner.write().await.remove(&user_id);
    
    tracing::info!("User {}'s WebSocket connection closed.", user_id);
}
