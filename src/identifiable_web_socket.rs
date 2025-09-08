use std::hash::{Hash, Hasher};
use serde_json::json;
use tokio::sync::mpsc;
use axum::extract::ws::Message;
use uuid::Uuid;

/// A wrapper around a WebSocket message sender that provides a unique ID.
/// This allows us to track a specific connection instance independently of the user.
#[derive(Clone, Debug)]
pub struct IdentifiableWebSocket {
    /// The unique identifier for this specific connection instance.
    pub id: Uuid,
    /// The channel sender used to send messages back to the client.
    pub sender: mpsc::Sender<Message>,
}

// Implement PartialEq and Eq based only on the ID
impl PartialEq for IdentifiableWebSocket {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for IdentifiableWebSocket {}

// Implement Hash based only on the ID
impl Hash for IdentifiableWebSocket {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl IdentifiableWebSocket {
    pub fn new(sender: mpsc::Sender<Message>) -> Self {
        Self {
            id: Uuid::new_v4(),
            sender,
        }
    }

    /// Primary function to send a WebSocket message.
    pub async fn send(&self, message: Message) -> Result<(), mpsc::error::SendError<Message>> {

        // Use the inner mpsc::Sender
        self.sender.send(message).await
    }

    /// Sends a simple JSON notification message to a specific connection.
    pub async fn notify_client(&self, message: &str) {
        let notification = json!({
            "notify": message
        });
        
        let send_result = self.send(Message::Text(notification.to_string().into())).await;
        
        if let Err(e) = send_result {
            tracing::error!("Failed to send notification to client {}: {}", self.id, e);
        }
    }
}