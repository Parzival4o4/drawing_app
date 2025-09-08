use std::{collections::{HashMap, HashSet}, path::PathBuf, sync::Arc};

use axum::extract::ws::Message;
use serde_json::json;
use sqlx::{query, SqlitePool};
use tokio::{fs::OpenOptions, sync::{Mutex, RwLock}};
use uuid::Uuid;
use tokio::io::AsyncWriteExt;

use crate::{identifiable_web_socket::IdentifiableWebSocket, websocket_handlers::WebSocketEvents, AppState};




// ============================= Structs (Unchanged from my previous reply) =============================

/// A struct that combines a user ID with an IdentifiableWebSocket.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConnectionInfo {
    pub user_id: i64,
    pub connection: IdentifiableWebSocket,
}

/// Helper struct for data retrieved from the Canvas DB table.
#[derive(Debug)]
pub struct CanvasDBInfo {
    pub file_path: PathBuf,
    pub is_moderated: bool,
}

#[derive(Debug)]
pub struct CanvasState {
    pub subscribers: HashSet<ConnectionInfo>,
    pub file_mutex: Arc<Mutex<()>>,
    pub is_moderated: bool,
    pub file_path: PathBuf,
}

impl CanvasState {
    /// Creates a new CanvasState from database info. (Kept simple/synchronous)
    pub fn new(info: CanvasDBInfo) -> Self {
        Self {
            subscribers: HashSet::new(),
            file_mutex: Arc::new(Mutex::new(())),
            file_path: info.file_path,
            is_moderated: info.is_moderated,
        }
    }
}

// ============================= Manager =============================

#[derive(Clone)]
pub struct CanvasManager {
    inner: Arc<RwLock<HashMap<String, CanvasState>>>,
}


#[derive(Debug)]
#[allow(dead_code)]
pub enum CanvasRegistrationError {
    NotFound,
    DatabaseError(String),
}

impl CanvasManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Helper function to find the file path and moderation state from the DB.
    /// This remains the source of truth for loading the initial state.
    async fn get_canvas_info(
        pool: &SqlitePool,
        canvas_uuid: &str,
    ) -> Result<CanvasDBInfo, CanvasRegistrationError> {
        let row = query!(
            "SELECT event_file_path, moderated FROM Canvas WHERE canvas_id = ?",
            canvas_uuid
        )
        .fetch_one(pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => CanvasRegistrationError::NotFound,
            _ => CanvasRegistrationError::DatabaseError(format!(
                "DB query failed for canvas {}: {}",
                canvas_uuid, e
            )),
        })?;

        Ok(CanvasDBInfo {
            file_path: PathBuf::from(row.event_file_path),
            is_moderated: row.moderated,
        })
    }


    // Helper function to read history and send moderation state first
    async fn send_canvas_history(
        connection: &IdentifiableWebSocket,
        file_path: &PathBuf,
        canvas_uuid: &str,
        is_moderated: bool,
        your_permission: &str,   
    ) {
        // 1. Send moderation state
        let moderated_msg = json!({
            "canvasId": canvas_uuid,
            "moderated": is_moderated
        });

        if let Err(e) = connection.send(Message::Text(moderated_msg.to_string().into())).await {
            tracing::error!("Failed to send moderation state to client {}: {}", connection.id, e);
        }

        // 2. Send history
        match tokio::fs::read_to_string(file_path).await {
            Ok(content) => {
                let mut events = Vec::new();

                for line in content.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }

                    match serde_json::from_str::<serde_json::Value>(line) {
                        Ok(value) => events.push(value),
                        Err(e) => {
                            tracing::warn!(
                                "Skipping invalid line in canvas {} history: {}",
                                canvas_uuid, e
                            );
                        }
                    }
                }

                let history_message = json!({
                    "canvasId": canvas_uuid,
                    "eventsForCanvas": events
                });

                if let Err(e) = connection.send(Message::Text(history_message.to_string().into())).await {
                    tracing::error!("Failed to send history to client {}: {}", connection.id, e);
                }
            }
            Err(_) => {
                connection
                    .notify_client("Failed to load canvas history. Try refreshing.")
                    .await;
            }
        }

        // 3. Send permission
        let permission_msg = json!({
            "canvasId": canvas_uuid,
            "yourPermission": your_permission
        });

        if let Err(e) = connection.send(Message::Text(permission_msg.to_string().into())).await {
            tracing::error!(
                "Failed to send permission to client {}: {}",
                connection.id,
                e
            );
        }
    }






    /// Registers a connection to a canvas.
    /// Returns an error only if there's a problem internal to the manager (e.g., lock poisoning).
    /// Sends a notification to the client if the canvas is not found in the DB.
    pub async fn register(
        &self,
        app_state: &AppState,
        canvas_uuid: String,
        user_id: i64,
        connection: IdentifiableWebSocket,
    ) {
        let connection_clone = connection.clone(); // Clone for error path and final insertion

        // === Check permissions before anything else ===
        let perm = app_state
            .socket_claims_manager
            .get_permission_level(user_id, &canvas_uuid.clone())
            .await;

        if perm.is_empty() {
            connection_clone
                .notify_client("You do not have permission to access this canvas.")
                .await;
            tracing::warn!(
                "User {} tried to register to canvas {} without permission",
                user_id,
                canvas_uuid
            );
            return;
        }

        // Acquire write lock on the manager's HashMap
        let mut manager_lock = self.inner.write().await;

        // Ensure canvas state exists in memory
        if !manager_lock.contains_key(&canvas_uuid) {
            tracing::info!("Canvas {} not in memory. Fetching info from DB.", canvas_uuid);

            // Attempt to load info from DB
            match Self::get_canvas_info(&app_state.pool, &canvas_uuid).await {
                Ok(db_info) => {
                    let new_state = CanvasState::new(db_info);
                    manager_lock.insert(canvas_uuid.clone(), new_state);
                }
                Err(CanvasRegistrationError::NotFound) => {
                    connection_clone
                        .notify_client(&format!(
                            "Canvas ID '{}' is invalid or does not exist.",
                            canvas_uuid
                        ))
                        .await;
                    tracing::error!("Canvas ID '{}' is invalid or does not exist.", canvas_uuid);
                    return;
                }
                Err(_) => {
                    connection_clone
                        .notify_client("A database error occurred. Cannot subscribe to canvas.")
                        .await;
                    tracing::error!("A database error occurred. Cannot subscribe to canvas.");
                    return;
                }
            }
        }

        // Now the state is guaranteed to exist
        let canvas_state = manager_lock
            .get_mut(&canvas_uuid)
            .expect("CanvasState must exist after check/insert.");

        let file_path = canvas_state.file_path.clone();

        // Add the connection info to the set.
        let connection_info = ConnectionInfo { user_id, connection };
        canvas_state.subscribers.insert(connection_info.clone());

        tracing::info!(
            "User {} subscribed to canvas {} (conn_id: {}). Total subscribers: {}. Moderated: {}",
            user_id,
            canvas_uuid,
            connection_info.connection.id,
            canvas_state.subscribers.len(),
            canvas_state.is_moderated,
        );

        // Send moderation, history, and permissions to the client
        Self::send_canvas_history(
            &connection_info.connection,
            &file_path,
            &canvas_uuid,
            canvas_state.is_moderated,
            &perm, 
        )
        .await;
    }



    /// Unregisters a specific connection from a canvas.
    pub async fn unregister_connection(
        &self,
        canvas_uuid: &str,
        conn_id: &Uuid,
    ) -> bool {
        let mut manager_lock = self.inner.write().await;

        if let Some(canvas_state) = manager_lock.get_mut(canvas_uuid) {
            let initial_len = canvas_state.subscribers.len();
            canvas_state.subscribers.retain(|info| &info.connection.id != conn_id);
            
            let was_removed = initial_len > canvas_state.subscribers.len();
            if was_removed {
                tracing::info!(
                    "Connection {} unsubscribed from canvas {}. Remaining subscribers: {}",
                    conn_id,
                    canvas_uuid,
                    canvas_state.subscribers.len()
                );
            }
            
            // Cleanup: If no more subscribers, remove the canvas from the map.
            if canvas_state.subscribers.is_empty() {
                manager_lock.remove(canvas_uuid);
                tracing::info!("Canvas {} removed from manager as it is now empty.", canvas_uuid);
            }
            was_removed
        } else {
            tracing::warn!("Attempted to unregister from a non-existent canvas: {}", canvas_uuid);
            false
        }
    }

    /// Unregisters all connections for a given user from a canvas.
    pub async fn unregister_user(
        &self,
        canvas_uuid: &str,
        user_id: i64,
    ) -> bool {
        let mut manager_lock = self.inner.write().await;

        if let Some(canvas_state) = manager_lock.get_mut(canvas_uuid) {
            let initial_len = canvas_state.subscribers.len();
            canvas_state.subscribers.retain(|info| info.user_id != user_id);
            
            let was_removed = initial_len > canvas_state.subscribers.len();
            if was_removed {
                tracing::info!(
                    "User {} unsubscribed all connections from canvas {}. Remaining subscribers: {}",
                    user_id,
                    canvas_uuid,
                    canvas_state.subscribers.len()
                );
            }
            
            if canvas_state.subscribers.is_empty() {
                manager_lock.remove(canvas_uuid);
                tracing::info!("Canvas {} removed from manager as it is now empty.", canvas_uuid);
            }
            was_removed
        } else {
            tracing::warn!("Attempted to unregister a user from a non-existent canvas: {}", canvas_uuid);
            false
        }
    }



    /// Handles an incoming event from a client, performing validation,
    /// permission checks, file writing, and broadcasting.
    ///
    /// The `sender_connection` is the specific WebSocket connection that sent the event.
    pub async fn handle_event(
        &self,
        state: &AppState,
        sender_id: i64,
        events: WebSocketEvents,
        original_message_text: String,
    ) {
        let canvas_uuid = &events.canvas_id;

        let manager_lock = self.inner.read().await;
        let canvas_state = if let Some(cs) = manager_lock.get(canvas_uuid) {
            cs
        } else {
            tracing::warn!(
                "Events received for canvas {} with no active manager entry. Dropping event.",
                canvas_uuid
            );
            return;
        };

        // 1. Permission Check
        let permission = state
            .socket_claims_manager
            .get_permission_level(sender_id, canvas_uuid)
            .await;

        let can_draw = matches!(permission.as_str(), "W" | "V" | "M" | "O" | "C");

        // If the canvas is moderated, "W" (Writer) permission is not enough to draw.
        let can_draw_in_moderated = can_draw && !canvas_state.is_moderated;
        let can_moderate = matches!(permission.as_str(), "M" | "O" | "C");
        let has_permission = can_draw_in_moderated || can_moderate;

        if !has_permission {
            tracing::warn!(
                "User {} denied drawing permission on canvas {}, their permission level is {}",
                sender_id,
                canvas_uuid,
                permission.as_str()
            );
            return;
        }

        // 2. Extract events_for_canvas
        let events_to_write = match events.events_for_canvas {
            serde_json::Value::Array(arr) => arr,
            _ => {
                tracing::error!("eventsForCanvas field is not an array.");
                return;
            }
        };

        // 3. Acquire File Mutex
        let file_path = &canvas_state.file_path;
        let lock_guard = canvas_state.file_mutex.lock().await;


        // 4. Write Events to File
        match OpenOptions::new().append(true).create(true).open(file_path).await {
            Ok(mut file) => {
                for event in events_to_write {
                    let event_line = event.to_string() + "\n";
                    if let Err(e) = file.write_all(event_line.as_bytes()).await {
                        tracing::error!(
                            "Failed to write event to file {}: {}",
                            file_path.display(),
                            e
                        );
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    "Failed to open/create file {}: {}",
                    file_path.display(),
                    e
                );
                return;
            }
        }
        drop(lock_guard);

        // 5. Broadcast the Original Message
        self.broadcast(canvas_uuid, Message::Text(original_message_text.into()))
            .await;
    }

    
    /// Sends a message to all active subscribers of a canvas.
    pub async fn broadcast(&self, canvas_uuid: &str, message: Message) {


        let map = self.inner.read().await;
        
        if let Some(canvas_state) = map.get(canvas_uuid) {
            let cloned_message = message.clone();
            
            for conn_info in canvas_state.subscribers.iter() {
                if let Err(e) = conn_info.connection.sender.send(cloned_message.clone()).await {
                    tracing::error!("Failed to send broadcast to conn {}: {}", conn_info.connection.id, e);
                }
            }
        } else {
            tracing::warn!("Attempted to broadcast to non-existent canvas: {}", canvas_uuid);
        }
    }

    pub async fn toggle_moderated_state(
        &self,
        state: &AppState,
        user_id: i64,
        canvas_uuid: String,
    ) {
        // 1. Check permissions
        let permission = state
            .socket_claims_manager
            .get_permission_level(user_id, &canvas_uuid)
            .await;

        let can_toggle = matches!(permission.as_str(), "M" | "O" | "C");
        if !can_toggle {
            tracing::warn!(
                "User {} denied moderation toggle on canvas {} (permission: {})",
                user_id,
                canvas_uuid,
                permission
            );
            return;
        }

        // 2. Acquire write lock on manager
        let mut map = self.inner.write().await;

        let canvas_state = if let Some(cs) = map.get_mut(&canvas_uuid) {
            cs
        } else {
            tracing::warn!(
                "toggle_moderated_state: Canvas {} not found in memory",
                canvas_uuid
            );
            return;
        };

        // Flip the moderation flag
        canvas_state.is_moderated = !canvas_state.is_moderated;
        let new_state = canvas_state.is_moderated;

        tracing::info!(
            "User {} toggled moderation for canvas {} -> {}",
            user_id,
            canvas_uuid,
            new_state
        );

        // 3. Update DB
        let moderated_value = if new_state { 1 } else { 0 };
        let update_res = query!(
            "UPDATE Canvas SET moderated = ? WHERE canvas_id = ?",
            moderated_value,
            canvas_uuid
        )
        .execute(&state.pool)
        .await;

        if let Err(e) = update_res {
            tracing::error!(
                "Failed to update moderated state for canvas {} in DB: {}",
                canvas_uuid,
                e
            );
            return;
        }

        // 4. Broadcast to all subscribers
        let msg = json!({
            "canvasId": canvas_uuid,
            "moderated": new_state
        });

        // Drop lock before broadcasting (avoid holding write lock while sending)
        drop(map);

        self.broadcast(&canvas_uuid, Message::Text(msg.to_string().into()))
            .await;
    }
}
