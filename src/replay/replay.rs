use std::time::Duration;

use tokio::{select, join};
use tokio_util::sync::CancellationToken;

use crate::{server::connection::Connection, accept::header::ConnectionType};

use super::receive::ReplayMerger;

pub struct Replay {
    id: u64,
    merger: ReplayMerger,
    shutdown_token: CancellationToken,
    replay_timeout_token: CancellationToken,
}

impl Replay {
    pub fn new(id: u64, shutdown_token: CancellationToken) -> Self {
        let replay_timeout_token = shutdown_token.child_token();
        let merger = ReplayMerger::new(replay_timeout_token.clone());
        Self { id, merger, shutdown_token, replay_timeout_token }
    }

    async fn timeout(&self) {
        let cancellation = async {
            // FIXME configure
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
            self.replay_timeout_token.cancel();
        };

        // Return early if we got cancelled normally
        select! {
            _ = cancellation => (),
            _ = self.replay_timeout_token.cancelled() => (),
        }
    }

    async fn regular_lifetime(&self) {
        self.merger.lifetime().await;
        // Cancel to notify timeout
        self.replay_timeout_token.cancel();
    }

    pub async fn lifetime(&self) {
        join! {
            self.regular_lifetime(),
            self.timeout(),
        };
    }

    pub async fn handle_connection(&self, mut c: Connection) -> ()
    {
        match c.get_header().type_ {
            ConnectionType::WRITER => self.merger.handle_connection(&mut c).await,
            ConnectionType::READER => todo!(),
        }
        // TODO
    }
}
