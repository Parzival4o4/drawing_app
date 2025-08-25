use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use crate::{auth::{get_claims, Claims, PartialClaims}, AppState};

// A tuple holding the user's claims and a counter for active connections
// The counter is essential because a single user can have multiple WebSocket connections.
pub type ClaimsCount = (Claims, usize);

#[derive(Clone)]
pub struct SocketClaimsManager {
    // Key: user_id (i64), Value: (Claims, connection_count)
    inner: Arc<RwLock<HashMap<i64, ClaimsCount>>>,
}

impl SocketClaimsManager {
    /// Creates a new, empty Claims Manager.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Adds a new connection for a user. If the user doesn't exist, their claims are added.
    /// If the user already exists, only the connection count is incremented.
    pub async fn add_connection_and_claims(&self, user_id: i64, claims: Claims) {
        let mut map = self.inner.write().await;
        
        // Check if the user ID is already in the map.
        if let Some((_, count)) = map.get_mut(&user_id) {
            // User exists, so we just increment the connection count.
            *count += 1;
            tracing::debug!("User {} connected again. Total connections: {}", user_id, *count);
        } else {
            // New user, so we insert the claims and set the count to 1.
            tracing::info!("First connection for user {}.", user_id);
            map.insert(user_id, (claims, 1));
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


    /// Refresh a user's claims by reloading permissions from the database.
    /// If the user is not connected, nothing happens.
    pub async fn update_permissions(&self, state: &AppState, user_id: i64) {
        tracing::info!("Permission update called for user: {}", user_id);

        // Grab the current claims (if the user is connected at all).
        let existing_claims = {
            let map = self.inner.read().await;
            map.get(&user_id).map(|(claims, _)| claims.clone())
        };

        if let Some(old_claims) = existing_claims {
            // Build a partial claims object to force a refresh of permissions.
            let partial_claims = PartialClaims {
                email: old_claims.email,
                user_id: Some(user_id),
                display_name: Some(old_claims.display_name),
                canvas_permissions: None, // this forces re-fetch
                ..PartialClaims::default()
            };

            // Fetch new claims from DB
            let updated_claims = match get_claims(&state.pool, partial_claims).await {
                Ok(claims) => claims,
                Err(e) => {
                    tracing::error!(
                        "Failed to get updated claims for user {}: {:?}",
                        user_id, e
                    );
                    return;
                }
            };

            // Update the claims in the in-memory map
            let mut map = self.inner.write().await;
            if let Some((claims, _count)) = map.get_mut(&user_id) {
                *claims = updated_claims;
                tracing::info!("Claims successfully refreshed for user {}", user_id);
            }
        } else {
            tracing::warn!("Permission update called for non-existent user {}", user_id);
        }
    }

    /// Removes a user's connection reference. If the count reaches zero, the claims are removed.
    pub async fn remove_connection(&self, user_id: i64) -> bool {
        let mut map = self.inner.write().await;
        
        if let Some((_, count)) = map.get_mut(&user_id) {
            *count -= 1;
            
            if *count == 0 {
                // Last connection closed, remove the entry
                map.remove(&user_id);
                tracing::info!("Last connection for user {} closed. Claims removed.", user_id);
                return true;
            } else {
                tracing::debug!("User {} connection closed. Remaining connections: {}", user_id, *count);
                return false;
            }
        } else {
            // Should not happen, but good for safety
            tracing::warn!("Attempted to remove connection for non-existent user {}", user_id);
            return false;
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


