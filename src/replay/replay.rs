use std::{time::Duration, cell::Cell};

use tokio::{select, join};
use tokio_util::sync::CancellationToken;

use crate::{server::connection::Connection, accept::header::ConnectionType, util::empty_counter::EmptyCounter};

use super::{receive::ReplayMerger, send::ReplaySender, save::ReplaySaver};

pub struct Replay {
    id: u64,
    merger: ReplayMerger,
    sender: ReplaySender,
    saver: ReplaySaver,
    replay_timeout_token: CancellationToken,
    writer_connection_count: EmptyCounter,
    reader_connection_count: EmptyCounter,
    should_stop_accepting_connections: Cell<bool>,
}

impl Replay {
    pub fn new(id: u64, shutdown_token: CancellationToken) -> Self {
        let writer_connection_count = EmptyCounter::new();
        let reader_connection_count = EmptyCounter::new();
        let should_stop_accepting_connections = Cell::new(false);
        let replay_timeout_token = shutdown_token.child_token();

        let merger = ReplayMerger::new(replay_timeout_token.clone());
        let merged_replay = merger.get_merged_replay();
        let sender = ReplaySender::new(merged_replay, replay_timeout_token.clone());
        let saver = ReplaySaver::new();

        Self { id, merger, sender, saver, replay_timeout_token,
               writer_connection_count, reader_connection_count,
               should_stop_accepting_connections }
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

    async fn wait_until_there_were_no_writers_for_a_while(&self) {
        // FIXME make configurable
        self.writer_connection_count.wait_until_empty_for(Duration::from_secs(30)).await;
    }

    async fn regular_lifetime(&self) {
        self.wait_until_there_were_no_writers_for_a_while().await;
        self.should_stop_accepting_connections.set(true);
        self.writer_connection_count.wait_until_empty().await;
        self.saver.save_replay(self.merger.get_merged_replay()).await;
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

    pub async fn handle_connection(&self, mut c: Connection) -> ()
    {
        if self.should_stop_accepting_connections.get() {
            return;     // TODO log
        }
        match c.get_header().type_ {
            ConnectionType::WRITER => {
                self.writer_connection_count.inc();
                self.merger.handle_connection(&mut c).await;
                self.writer_connection_count.dec();
            },
            ConnectionType::READER => {
                self.reader_connection_count.inc();
                self.sender.handle_connection(&mut c).await;
                self.reader_connection_count.dec();
            }
        }
        // TODO
    }
}
