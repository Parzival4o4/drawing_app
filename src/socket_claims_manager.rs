use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use crate::{auth::{get_claims, Claims, PartialClaims}, identifiable_web_socket::IdentifiableWebSocket, AppState};
use serde_json::json;
use axum::extract::ws::Message;

// A tuple holding the user's claims and a list of their active connections
pub type ClaimsConnections = (Claims, Vec<IdentifiableWebSocket>);

#[derive(Clone)]
pub struct SocketClaimsManager {
    // Key: user_id (i64), Value: (Claims, Vec<IdentifiableWebSocket>)
    inner: Arc<RwLock<HashMap<i64, ClaimsConnections>>>,
}

impl SocketClaimsManager {
    /// Creates a new, empty Claims Manager.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Adds a new connection for a user. If the user doesn't exist, their claims are added.
    pub async fn add_connection_and_claims(&self, user_id: i64, claims: Claims, ws: IdentifiableWebSocket) {
        let mut map = self.inner.write().await;
        
        // Check if the user ID is already in the map.
        if let Some((_, connections)) = map.get_mut(&user_id) {
            // User exists, so we just add the new connection to their list.
            connections.push(ws);
            tracing::debug!("User {} connected again. Total connections: {}", user_id, connections.len());
        } else {
            // New user, so we insert the claims and the new connection.
            tracing::info!("First connection for user {}.", user_id);
            map.insert(user_id, (claims, vec![ws]));
        }
    }

    /// Updates an existing user's claims. This is useful for permission refreshes.
    /// This function will not change the connection count.
    pub async fn update_claims(&self, user_id: i64, updated_claims: Claims) -> bool {
        let mut map = self.inner.write().await;
        if let Some((existing_claims, _)) = map.get_mut(&user_id) {
            *existing_claims = updated_claims;
            tracing::info!("Claims updated for user {}.", user_id);
            true
        } else {
            tracing::warn!("Failed to update claims for non-existent user {}.", user_id);
            false
        }
    }

    /// Refresh a user's permissions and send an update message to all their active connections.
    pub async fn update_permissions(&self, state: &AppState, user_id: i64) {
        tracing::info!("Permission update called for user: {}", user_id);

        let mut write_map = self.inner.write().await;

        if let Some((old_claims, connections)) = write_map.get_mut(&user_id) {
            // Build a partial claims object to force a refresh of permissions.
            let partial_claims = PartialClaims {
                email: old_claims.email.clone(),
                user_id: Some(user_id),
                display_name: Some(old_claims.display_name.clone()),
                canvas_permissions: None, // this forces re-fetch
                ..PartialClaims::default()
            };

            let updated_claims = match get_claims(&state.pool, partial_claims).await {
                Ok(claims) => claims,
                Err(e) => {
                    tracing::error!("Failed to get updated claims for user {}: {:?}", user_id, e);
                    return;
                }
            };
            
            // Update the claims in the in-memory map
            *old_claims = updated_claims.clone();
            tracing::info!("Claims successfully refreshed for user {}", user_id);

            // Send the new permission to all active connections
            for ws in connections.iter() {
                for (canvas_id, new_permission) in &updated_claims.canvas_permissions {
                    let message = json!({
                        "canvasId": canvas_id,
                        "yourPermission": new_permission,
                    });
                    
                    if let Err(e) = ws.send(Message::Text(message.to_string().into())).await {
                        tracing::error!("Failed to send permission update to client {}: {}", ws.id, e);
                    }
                }
            }
        } else {
            tracing::warn!("Permission update called for non-existent user {}", user_id);
        }
    }

    /// Removes a user's connection reference. If the connection is the last one for a user, the entry is removed.
    pub async fn remove_connection(&self, user_id: i64, ws_to_remove: &IdentifiableWebSocket) -> bool {
        let mut map = self.inner.write().await;
        
        if let Some((_, connections)) = map.get_mut(&user_id) {
            // Remove the specific WebSocket connection from the vector.
            connections.retain(|ws| ws.id != ws_to_remove.id);
            
            if connections.is_empty() {
                // Last connection closed, remove the entry
                map.remove(&user_id);
                tracing::info!("Last connection for user {} closed. Claims removed.", user_id);
                true
            } else {
                tracing::debug!("User {} connection closed. Remaining connections: {}", user_id, connections.len());
                false
            }
        } else {
            // Should not happen, but good for safety
            tracing::warn!("Attempted to remove connection for non-existent user {}", user_id);
            false
        }
    }

    /// Retrieves the permission level for a user on a specific canvas.
    /// Returns the permission string or an empty string if not found.
    pub async fn get_permission_level(&self, user_id: i64, canvas_id: &str) -> String {
        let map = self.inner.read().await;
        
        // Use a chain of option methods to safely get the permission
        map.get(&user_id)
            .and_then(|(claims, _)| {
                claims.canvas_permissions.get(canvas_id)
            })
            .cloned() // Clone the string to return it
            .unwrap_or_else(|| {
                // Return an empty string if no permission is found
                "".to_string()
            })
    }
}