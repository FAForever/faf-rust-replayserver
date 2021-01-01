use tokio_util::sync::CancellationToken;

use crate::server::connection::Connection;

pub struct Replay {
    id: u64,
    shutdown_token: CancellationToken,
}

impl Replay {
    pub fn new(id: u64, shutdown_token: CancellationToken) -> Self {
        Self { id, shutdown_token }
    }
    pub async fn lifetime(&self) -> () {
        // TODO
    }
    pub async fn handle_connection(&self, c: Connection) -> () {
        // TODO
    }
}
