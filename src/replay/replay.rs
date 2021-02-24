use std::{cell::Cell, time::Duration};

use tokio::join;
use tokio_util::sync::CancellationToken;

use crate::{
    accept::header::ConnectionType,
    config::Settings,
    server::connection::Connection,
    util::{empty_counter::EmptyCounter, timeout::cancellable},
};

use super::{receive::ReplayMerger, save::ReplaySaver, send::ReplaySender};

pub struct Replay {
    id: u64,
    merger: ReplayMerger,
    sender: ReplaySender,
    saver: ReplaySaver,
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
        config: Settings,
        saver: ReplaySaver,
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
        cancellable(cancellation, &self.replay_timeout_token).await;
    }

    async fn wait_until_there_were_no_writers_for_a_while(&self) {
        let wait = self.writer_connection_count
            .wait_until_empty_for(self.time_with_zero_writers_to_end_replay);
        // We don't have to return when there are no writers, just when we shouldn't accept more.
        cancellable(wait, &self.replay_timeout_token).await;
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

#[cfg(test)]
mod test {
    use std::sync::{Arc, Once};

    use super::*;
    use crate::{accept::header::ConnectionHeader, config::test::default_config, replay::save::InnerReplaySaver, server::connection::test::test_connection};

    // TODO - copypasta. Initialize logging before tests pls.
    static INIT: Once = Once::new();
    fn setup() {
        INIT.call_once(env_logger::init);
    }

    #[tokio::test]
    async fn test_replay_forced_timeout() {
        setup();
        tokio::time::pause();

        let mut mock_saver = InnerReplaySaver::faux();
        faux::when!(mock_saver.save_replay).then_do(|| ());

        let token = CancellationToken::new();
        let mut config = default_config();
        config.replay.forced_timeout_s = 3600;

        let (mut c, _r, _w) = test_connection();
        let c_header = ConnectionHeader {
            type_: ConnectionType::WRITER,
            id: 1,
            name: "foo".into(),
        };
        c.set_header(c_header);

        let replay = Replay::new(1, token, Arc::new(config), Arc::new(mock_saver));

        let replay_ended = Cell::new(false);
        let run_replay = async {
            join! {
                replay.lifetime(),
                replay.handle_connection(c),
            };
            replay_ended.set(true);
        };
        let check_result = async {
            tokio::time::sleep(Duration::from_secs(3599)).await;
            assert!(!replay_ended.get());
            tokio::time::sleep(Duration::from_secs(2)).await;
            assert!(replay_ended.get());
        };

        join! { run_replay, check_result };
    }
}
