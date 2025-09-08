use axum::{extract::{ws::{Message, WebSocket}, State, WebSocketUpgrade}, response::IntoResponse};
use futures::StreamExt;
use std::collections::HashSet;
use tokio::sync::mpsc;
use crate::auth::{get_claims, Claims, PartialClaims};
use crate::AppState;
use serde::{Deserialize, Serialize};
use crate::identifiable_web_socket::IdentifiableWebSocket;
use futures::SinkExt; // needed for sender.send(...)


// ============================= message Struct =============================

#[derive(Serialize, Deserialize)]
pub struct WebSocketEvents {
    #[serde(rename = "canvasId")]
    pub canvas_id: String,
    #[serde(rename = "eventsForCanvas")]
    pub events_for_canvas: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
pub struct WebSocketCommand {
    pub command: String,
    #[serde(rename = "canvasId")]
    pub canvas_id: String,
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
    
    // Create the IdentifiableWebSocket before adding the connection
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::channel::<Message>(128);
    let id_socket = IdentifiableWebSocket::new(tx);

    // Add the IdentifiableWebSocket to the claims manager
    state.socket_claims_manager.add_connection_and_claims(user_id, claims, id_socket.clone()).await;

    tracing::info!("User {} connected via WebSocket.", user_id);

    // Spawn a task to forward messages from the channel to the WebSocket sink
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = sender.send(msg).await {
                tracing::error!("Failed to send message to client: {}", e);
                break;
            }
        }
    });

    // Track canvases this connection has subscribed to
    let mut subscribed_canvases = HashSet::<String>::new();

    // Handle incoming messages loop
    handle_incoming_messages(
        user_id,
        &mut receiver,
        &state,
        id_socket.clone(),
        &mut subscribed_canvases,
    )
    .await;

    // Cleanup
    tracing::info!(
        "User {}'s WebSocket connection closed. Unsubscribing from {} canvases.",
        user_id,
        subscribed_canvases.len()
    );

    for canvas_id in subscribed_canvases.drain() {
        state
            .canvas_manager
            .unregister_connection(&canvas_id, &id_socket.id)
            .await;
    }

    // Remove the IdentifiableWebSocket from the claims manager
    state.socket_claims_manager.remove_connection(user_id, &id_socket).await;

    tracing::info!("User {}'s WebSocket connection cleanup complete.", user_id);
}



async fn handle_incoming_messages(
    user_id: i64,
    receiver: &mut futures::stream::SplitStream<WebSocket>,
    state: &AppState,
    id_socket: IdentifiableWebSocket,
    subscribed_canvases: &mut HashSet<String>,
) {
    loop {
        tokio::select! {
            Some(Ok(message)) = receiver.next() => {
                match message {
                    Message::Text(text) => {
                        tracing::info!("Received message from user {}: {}", user_id, text);

                        if let Err(e) = process_command(
                            user_id,
                            text.to_string(),
                            state,
                            id_socket.clone(),
                            subscribed_canvases
                        ).await {
                            tracing::error!("Failed to process command for user {}: {}", user_id, e);
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

async fn process_command(
    user_id: i64,
    text: String,
    state: &AppState,
    id_socket: IdentifiableWebSocket,
    subscribed_canvases: &mut HashSet<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Ok(events) = serde_json::from_str::<WebSocketEvents>(&text) {
        tracing::info!("Processing WebSocketEvents for canvas {}", events.canvas_id);

        if !events.events_for_canvas.is_array() {
            tracing::warn!("eventsForCanvas was not an array for user {} on canvas {}", user_id, events.canvas_id);
            return Ok(());
        }

        state.canvas_manager.handle_event(state, user_id, events, text).await;
        return Ok(());
    }

    if let Ok(cmd) = serde_json::from_str::<WebSocketCommand>(&text) {
        tracing::info!("Processing WebSocketCommand '{}' for canvas {}", cmd.command, cmd.canvas_id);

        match cmd.command.as_str() {
            "registerForCanvas" => {
                state.canvas_manager.register(state, cmd.canvas_id.clone(), user_id, id_socket.clone()).await;
                subscribed_canvases.insert(cmd.canvas_id.clone());
                tracing::info!("User {} subscribed to canvas {}", user_id, cmd.canvas_id);
            }
            "unregisterForCanvas" => {
                state.canvas_manager.unregister_connection(&cmd.canvas_id, &id_socket.id).await;
                subscribed_canvases.remove(&cmd.canvas_id);
                tracing::info!("User {} unsubscribed from canvas {}", user_id, cmd.canvas_id);
            }
            "toggleModerated" => {
                state.canvas_manager.toggle_moderated_state(state, user_id, cmd.canvas_id.clone()).await;
                tracing::info!("User {} toggled moderation on canvas {}", user_id, cmd.canvas_id);
            }
            _ => {
                tracing::warn!("Unknown WebSocketCommand '{}' from user {}", cmd.command, user_id);
            }
        }

        return Ok(());
    }

    tracing::warn!("Failed to parse incoming message from user {}: {}", user_id, text);
    Ok(())
}
