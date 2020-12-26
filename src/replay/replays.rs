use log::info;
use tokio::sync::mpsc::Receiver;
use tokio_util::sync::CancellationToken;

use crate::server::connection::Connection;

pub struct Replays {
    shutdown_token: CancellationToken,
    connections: Receiver<Connection>,
}

impl Replays {
    pub fn new(shutdown_token: CancellationToken,
               connections: Receiver<Connection>) -> Self
    {
        Replays {shutdown_token, connections}
    }

    pub async fn lifetime(&mut self) {
        info!("TODO handle connections properly");
        while let Some(conn) = self.connections.recv().await {
            let _ = conn;
        }
    }
}
