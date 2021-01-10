use tokio_util::sync::CancellationToken;

use crate::server::connection::Connection;

pub struct ReplayMerger {
    shutdown_token: CancellationToken,
}

impl ReplayMerger {
    pub fn new(shutdown_token: CancellationToken) -> Self {
        Self {shutdown_token}
    }
    pub async fn lifetime(&self) {
        // TODO
    }
    pub async fn handle_connection(&self, c: Connection) {
        // TODO
    }
}
