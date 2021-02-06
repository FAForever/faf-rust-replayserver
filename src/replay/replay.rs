use std::{cell::Cell, sync::Arc, time::Duration};

use tokio::{join, select};
use tokio_util::sync::CancellationToken;

use crate::{
    accept::header::ConnectionType, config::Settings, server::connection::Connection,
    util::empty_counter::EmptyCounter,
};

use super::{receive::ReplayMerger, save::ReplaySaver, send::ReplaySender};

pub struct Replay {
    id: u64,
    merger: ReplayMerger,
    sender: ReplaySender,
    saver: Arc<ReplaySaver>,
    replay_timeout_token: CancellationToken,
    writer_connection_count: EmptyCounter,
    reader_connection_count: EmptyCounter,
    time_with_zero_writers_to_end_replay: Duration,
    forced_timeout: Duration,
    should_stop_accepting_connections: Cell<bool>,
}

impl Replay {
    pub fn new(
        id: u64,
        shutdown_token: CancellationToken,
        config: &Settings,
        saver: Arc<ReplaySaver>,
    ) -> Self {
        let writer_connection_count = EmptyCounter::new();
        let reader_connection_count = EmptyCounter::new();
        let should_stop_accepting_connections = Cell::new(false);
        let time_with_zero_writers_to_end_replay =
            Duration::from_secs(config.replay.time_with_zero_writers_to_end_replay_s);
        let forced_timeout = Duration::from_secs(config.replay.forced_timeout_s);
        let replay_timeout_token = shutdown_token.child_token();

        let merger = ReplayMerger::new(replay_timeout_token.clone(), config);
        let merged_replay = merger.get_merged_replay();
        let sender = ReplaySender::new(merged_replay, replay_timeout_token.clone());

        Self {
            id,
            merger,
            sender,
            saver,
            replay_timeout_token,
            writer_connection_count,
            reader_connection_count,
            time_with_zero_writers_to_end_replay,
            forced_timeout,
            should_stop_accepting_connections,
        }
    }

    async fn timeout(&self) {
        let cancellation = async {
            tokio::time::sleep(self.forced_timeout).await;
            self.replay_timeout_token.cancel();
        };

        // Return early if we got cancelled normally
        select! {
            _ = cancellation => (),
            _ = self.replay_timeout_token.cancelled() => (),
        }
    }

    async fn wait_until_there_were_no_writers_for_a_while(&self) {
        self.writer_connection_count
            .wait_until_empty_for(self.time_with_zero_writers_to_end_replay)
            .await;
    }

    async fn regular_lifetime(&self) {
        self.wait_until_there_were_no_writers_for_a_while().await;
        self.should_stop_accepting_connections.set(true);
        self.writer_connection_count.wait_until_empty().await;
        self.merger.finalize();
        self.saver
            .save_replay(self.merger.get_merged_replay(), self.id)
            .await;
        self.reader_connection_count.wait_until_empty().await;
        // Cancel to return from timeout
        self.replay_timeout_token.cancel();
    }

    pub async fn lifetime(&self) {
        join! {
            self.regular_lifetime(),
            self.timeout(),
        };
    }

    pub async fn handle_connection(&self, mut c: Connection) -> () {
        if self.should_stop_accepting_connections.get() {
            log::info!(
                "Replay {} dropped a connection because its write phase is over",
                self.id
            );
            return;
        }
        match c.get_header().type_ {
            ConnectionType::WRITER => {
                self.writer_connection_count.inc();
                self.merger.handle_connection(&mut c).await;
                self.writer_connection_count.dec();
            }
            ConnectionType::READER => {
                self.reader_connection_count.inc();
                self.sender.handle_connection(&mut c).await;
                self.reader_connection_count.dec();
            }
        }
    }
}
