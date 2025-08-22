// src/handlers.rs
// Add these imports to your handlers.rs
use axum::{extract::{ws::{Message, WebSocket}, State, WebSocketUpgrade}, response::IntoResponse};
use futures::StreamExt;
use std::{collections::{HashMap, HashSet}, sync::Arc};
use tokio::sync::{mpsc, RwLock};
use crate::auth::{get_claims, Claims, PartialClaims};
use crate::AppState; // Import AppState
use futures::SinkExt;
use serde::{Deserialize, Serialize};

// ============================= Command Struct =============================

#[derive(Serialize, Deserialize)]
pub struct WebSocketCommand {
    pub command: Option<String>,
    #[serde(rename = "canvasId")]
    pub canvas_id: Option<String>,
    #[serde(rename = "eventsForCanvas")]
    pub events_for_canvas: Option<serde_json::Value>,
}

// ============================= active connections =============================

// A struct to hold the sender and the claims for each connection.
#[derive(Debug)]
pub struct WebSocketConnection {
    pub user_claims: Claims,
    pub message_sender: mpsc::Sender<Message>,
    pub subscribed_canvases: HashSet<String>,
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
        } else {
            tracing::debug!("User {} not found in active connections. No claims to update.", user_id);
            false
        }
    }
}


// ============================= canvas subscriptions =============================

// New struct to manage a single canvas's subscribers and state
#[derive(Debug)]
pub struct CanvasState {
    pub is_moderated: bool,
    // A list of subscribed user IDs.
    pub subscribers: HashSet<i64>,
}

// A new struct to encapsulate all canvas subscriptions
#[derive(Clone)]
pub struct CanvasManager {
    // A map from canvas UUID to the CanvasState
    pub inner: Arc<RwLock<HashMap<String, CanvasState>>>,
}

impl CanvasManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get_permission(&self, user_id: i64, canvas_uuid: &str, state: &AppState) -> Option<String> {
        let active_connections = state.active_connections.inner.read().await;
        if let Some(conn) = active_connections.get(&user_id) {
            conn.user_claims.canvas_permissions.get(canvas_uuid).cloned()
        } else {
            None
        }
    }

    pub async fn register_for_canvas(&self, state: &AppState, user_id: i64, canvas_uuid: String) -> Result<(), String> {
        // 1. Check if the client has permissions
        let permission = self.get_permission(user_id, &canvas_uuid, state).await;
        // if permission.is_none() {
        //     return Err("Permission denied".to_string());
        // }

        // 2. Add the canvas to the WebSocketConnection's list
        let mut active_connections_lock = state.active_connections.inner.write().await;
        if let Some(conn) = active_connections_lock.get_mut(&user_id) {
            conn.subscribed_canvases.insert(canvas_uuid.clone());
        } else {
            return Err("User's connection not found".to_string());
        }

        // 3. Update the CanvasManager
        let mut canvas_manager_lock = self.inner.write().await;
        let canvas_state = canvas_manager_lock
            .entry(canvas_uuid)
            .or_insert_with(|| CanvasState {
                is_moderated: false,
                subscribers: HashSet::new(),
            });

        // Add the user to the canvas subscribers
        canvas_state.subscribers.insert(user_id);

        Ok(())
    }

    pub async fn broadcast_to_canvas(&self, state: &AppState, sender_id: i64, canvas_uuid: String, message: Message) {
        // Check if the sender has permission to draw on the canvas
        // let permission = self.get_permission(sender_id, &canvas_uuid, state).await;
        tracing::debug!("broadcasting");
        let permission = Some("O");
        let can_draw = match permission.as_deref() {
            Some("W") | Some("V") | Some("M") | Some("O") | Some("C") => true,
            _ => false,
        };
        
        // Check for moderated state for "W" permission
        let is_moderated = {
            let canvas_manager_lock = self.inner.read().await;
            canvas_manager_lock.get(&canvas_uuid).map_or(false, |cs| cs.is_moderated)
        };
        
        if !can_draw || (is_moderated && matches!(permission.as_deref(), Some("W"))) {
            tracing::warn!("User {} tried to draw on canvas {} without sufficient permission or due to moderation.", sender_id, canvas_uuid);
            // TODO: send a specific error message back to the sender
            return;
        }

        let canvas_manager_lock = self.inner.read().await;
        if let Some(canvas_state) = canvas_manager_lock.get(&canvas_uuid) {
            let active_connections_lock = state.active_connections.inner.read().await;
            tracing::debug!("broadcasting2");
            for &subscriber_id in &canvas_state.subscribers {
                tracing::debug!("broadcasting3");
                if  true { //subscriber_id != sender_id {
                    if let Some(conn) = active_connections_lock.get(&subscriber_id) {
                        if let Err(e) = conn.message_sender.send(message.clone()).await {
                            tracing::error!("Failed to send broadcast to user {}: {}", subscriber_id, e);
                        }
                    }
                }
            }
        }
    }
}


// ============================= handlers =============================

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    mut claims: Claims,
    State(state): State<AppState>,
) -> impl IntoResponse {

    let now = jsonwebtoken::get_current_timestamp() as usize;

    let soft_expired = claims.reissue_time <= now;
    let refresh_list_entry = state.permission_refresh_list.has_pending_refresh(claims.user_id).await;

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

    ws.on_upgrade(move |socket| handle_websocket(socket, claims, state))
}


async fn handle_websocket(socket: WebSocket, claims: Claims, state: AppState) {
    let user_id = claims.user_id;
    let (mut sender, mut receiver) = socket.split();

    let (tx, mut rx) = mpsc::channel::<Message>(128);

    let wrapped_connection = WebSocketConnection {
        user_claims: claims,
        message_sender: tx,
        subscribed_canvases: HashSet::new(),
    };
    state.active_connections.inner.write().await.insert(user_id, wrapped_connection);
    
    // Unused variable, so we add _ to the name. We also remove mut as it's not needed.
    let _sender_task = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if let Err(e) = sender.send(message).await {
                tracing::error!("Failed to send message to user {}: {}", user_id, e);
                break;
            }
        }
    });

    tracing::info!("User {} connected via WebSocket.", user_id);

    // Call a helper function to handle the loop
    handle_incoming_messages(user_id, &mut receiver, &state).await;
    
    // Unsubscription and cleanup logic after the loop exits
    tracing::info!("User {}'s WebSocket connection closed. Removing from maps.", user_id);

    let subscribed_canvases_to_remove: Vec<String> = {
        let active_connections_lock = state.active_connections.inner.read().await;
        if let Some(conn) = active_connections_lock.get(&user_id) {
            conn.subscribed_canvases.iter().cloned().collect()
        } else {
            vec![]
        }
    };

    let mut canvas_manager_lock = state.canvas_manager.inner.write().await;
    for canvas_uuid in subscribed_canvases_to_remove {
        if let Some(canvas_state) = canvas_manager_lock.get_mut(&canvas_uuid) {
            canvas_state.subscribers.remove(&user_id);
            if canvas_state.subscribers.is_empty() {
                canvas_manager_lock.remove(&canvas_uuid);
                tracing::info!("Canvas {} removed from manager as it is now empty.", canvas_uuid);
            }
        }
    }
    
    state.active_connections.inner.write().await.remove(&user_id);
    
    tracing::info!("User {}'s WebSocket connection cleanup complete.", user_id);
}

// New helper function to encapsulate the loop logic
async fn handle_incoming_messages(user_id: i64, receiver: &mut futures::stream::SplitStream<WebSocket>, state: &AppState) {
    loop {
        tokio::select! {
            Some(Ok(message)) = receiver.next() => {
                match message {
                    Message::Text(text) => {
                        // Log the received message before processing it.
                        tracing::info!("Received message from user {}: {}", user_id, text);
                        
                        // We clone here to pass ownership to the new function,
                        // while keeping the original string for potential reuse below.
                        // This resolves the first error.
                        if let Err(e) = process_command(user_id, text.to_string(), state).await {
                            tracing::error!("Failed to process command for user {}: {}", user_id, e);
                            // TODO: Consider sending an error message back to the client
                        }
                    }
                    Message::Close(_) => {
                        tracing::info!("User {} sent a close frame. Exiting loop.", user_id);
                        break;
                    }
                    _ => {}
                }
            }
            else => {
                break;
            }
        }
    }
}

// New helper function to process a single command
async fn process_command(user_id: i64, text: String, state: &AppState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // We try to parse the incoming message as a WebSocketCommand
    let command: WebSocketCommand = serde_json::from_str(&text)?;

    // We check for the new "eventsForCanvas" field first, as that is the primary
    // way of sending a list of events.
    if let Some(canvas_uuid) = command.canvas_id {
        if let Some(_) = command.events_for_canvas {
             let active_connections_lock = state.active_connections.inner.read().await;
            if let Some(conn) = active_connections_lock.get(&user_id) {
                if conn.subscribed_canvases.contains(&canvas_uuid) {
                    state.canvas_manager.broadcast_to_canvas(state, user_id, canvas_uuid, Message::Text(text.into())).await;
                } else {
                    tracing::warn!("User {} tried to add events to a canvas they are not subscribed to: {}", user_id, canvas_uuid);
                    // TODO: send an error message to the client
                }
            }
        } else if command.command.as_deref() == Some("registerForCanvas") {
            tracing::info!("User {} wants to subscribe to canvas {}", user_id, canvas_uuid);
            state.canvas_manager.register_for_canvas(state, user_id, canvas_uuid).await?;
            tracing::info!("User {} successfully subscribed to canvas", user_id);
        } else {
            tracing::warn!("Received unknown command or malformed message from user {}: {}", user_id, text);
        }
    } else {
         tracing::warn!("Received message without a canvasId from user {}: {}", user_id, text);
    }
    
    Ok(())
}
