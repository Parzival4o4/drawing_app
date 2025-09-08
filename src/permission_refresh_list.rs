
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::auth::REISSUE_AFTER_SECONDS;


// As far as I can tell, there is no way to implement timely permission updates in users' JWTs without accessing server-side state on each user request.
// It is possible to do so without server state only if JWTs expire after a fixed interval.
// However, this approach either causes permission updates to take minutes to propagate 
// or requires frequently reissuing JWTs because of a short expiry time.
//
// Note that pushing updates through WebSockets alone is insufficient,
// because a user might not be connected to the web app but still possess a valid token.
//
// I believe I have found a good hybrid solution:
// Whenever changes are made to a user's permissions, an entry is added to a server-side hash map.
// When that user makes a request, the map is checked for an entry corresponding to the user.
// If such an entry exists, the user's JWT is refreshed before handling the request.
//
// To prevent the hash map from growing uncontrollably over time,
// JWTs have a reissue time of 5 minutes.
// If a request arrives with a JWT older than the reissue time, the server issues a new token valid for another 5 minutes.
// This means we can safely prune all entries from the hash map older than 5 minutes.
//
// Access to this server state is efficient (constant time complexity) and fast because it is stored in memory.
// Space complexity remains bounded due to the automatic pruning mechanism.


type UserId = i64;

#[derive(Clone)]
pub struct PermissionRefreshList {
    inner: Arc<RwLock<HashMap<UserId, usize>>>,
}

impl PermissionRefreshList {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    pub async fn mark_user_for_refresh(&self, user_id: UserId) {
        let now = current_timestamp();
        let mut map = self.inner.write().await;
        map.insert(user_id, now);
    }
    pub async fn consume_refresh_request(&self, user_id: UserId) -> bool {
        let mut map = self.inner.write().await;
        if map.remove(&user_id).is_some() {
            true
        } else {
            false
        }
    }
    pub async fn has_pending_refresh(&self, user_id: UserId) -> bool {
        let map = self.inner.read().await;
        map.contains_key(&user_id)
    }
    pub async fn prune_old_entries(&self, max_age: usize) {
        let now = current_timestamp();
        let mut map = self.inner.write().await;
        map.retain(|_, &mut timestamp| now < timestamp + max_age);
    }
}

fn current_timestamp() -> usize {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize
}

pub async fn start_cleanup_task(refresh_list: Arc<PermissionRefreshList>) {
    let reissue_time: usize = REISSUE_AFTER_SECONDS;
    let prune_age = reissue_time * 2;
    let interval = Duration::from_secs(reissue_time as u64);

    loop {
        sleep(interval).await;
        tracing::debug!("running refresh List prune");
        refresh_list.prune_old_entries(prune_age).await;
        tracing::debug!("done with refresh List prune");
    }
}