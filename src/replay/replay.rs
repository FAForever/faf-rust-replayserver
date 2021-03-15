use std::cell::Cell;

use tokio::join;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

use super::{receive::ReplayMerger, save::ReplaySaver, send::ReplaySender};
use crate::error::ConnectionError;
use crate::{
    accept::header::ConnectionType,
    config::Settings,
    error::ConnResult,
    metrics,
    server::connection::Connection,
    util::{empty_counter::EmptyCounter, timeout::cancellable},
};

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
            log::info!("Replay {} timed out", self.id);
        };

        // Return early if we got cancelled normally
        cancellable(cancellation, &self.replay_timeout_token).await;
    }

    async fn wait_until_there_were_no_writers_for_a_while(&self) {
        let wait = self
            .writer_connection_count
            .wait_until_empty_for(self.time_with_zero_writers_to_end_replay);
        // We don't have to return when there are no writers, just when we shouldn't accept more.
        cancellable(wait, &self.replay_timeout_token).await;
    }

    async fn regular_lifetime(&self) {
        log::info!("Replay {} started", self.id);
        metrics::RUNNING_REPLAYS.inc();
        self.wait_until_there_were_no_writers_for_a_while().await;
        self.should_stop_accepting_connections.set(true);
        log::debug!("Replay {} stopped accepting connections", self.id);
        self.writer_connection_count.wait_until_empty().await;
        self.merger.finalize();
        log::debug!("Replay {} finished merging data", self.id);
        self.saver
            .save_replay(self.merger.get_merged_replay(), self.id)
            .await;
        log::debug!("Replay {} saved data on disk", self.id);
        self.reader_connection_count.wait_until_empty().await;
        log::info!("Replay {} ended", self.id);
        // Cancel to return from timeout
        self.replay_timeout_token.cancel();
        metrics::RUNNING_REPLAYS.dec();
        metrics::FINISHED_REPLAYS.inc();
    }

    pub async fn lifetime(&self) {
        join! {
            self.regular_lifetime(),
            self.timeout(),
        };
    }

    pub async fn handle_connection(&self, mut c: Connection) -> ConnResult<()> {
        log::debug!("Replay {} started handling {}", self.id, c);
        if self.should_stop_accepting_connections.get() {
            log::info!(
                "Replay {} dropped {} because its write phase is over",
                self.id,
                c
            );
            return Err(ConnectionError::CannotAssignToReplay);
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
        log::debug!("Replay {} finished handling {}", self.id, c);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::util::test::sleep_s;
    use crate::{
        accept::header::ConnectionHeader,
        config::test::default_config,
        replay::save::InnerReplaySaver,
        server::connection::test::test_connection,
        util::test::{compare_bufs, get_file, setup_logging},
    };

    #[tokio::test]
    async fn test_replay_forced_timeout() {
        setup_logging();
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
            (join! {
                replay.lifetime(),
                replay.handle_connection(c),
            })
            .1
            .unwrap();
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

    #[tokio::test]
    async fn test_replay_one_writer_one_reader() {
        setup_logging();
        tokio::time::pause();

        let mut mock_saver = InnerReplaySaver::faux();
        faux::when!(mock_saver.save_replay).then_do(|| ());
        let token = CancellationToken::new();
        let config = default_config();

        let (mut c_read, mut reader, mut _w) = test_connection();
        let (mut c_write, _r, mut writer) = test_connection();
        c_write.set_header(ConnectionHeader {
            type_: ConnectionType::WRITER,
            id: 1,
            name: "foo".into(),
        });
        c_read.set_header(ConnectionHeader {
            type_: ConnectionType::READER,
            id: 1,
            name: "foo".into(),
        });

        let replay = Replay::new(1, token, Arc::new(config), Arc::new(mock_saver));
        let run_replay = async {
            (join! {
                replay.lifetime(),
                replay.handle_connection(c_write),
                async {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    replay.handle_connection(c_read).await.unwrap();
                }
            })
            .1
            .unwrap();
        };

        let example_replay_file = get_file("example");
        let replay_writing = async {
            for data in example_replay_file.chunks(100) {
                writer.write_all(data).await.unwrap();
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            drop(writer);
        };
        let mut received_replay_file = Vec::<u8>::new();
        let replay_reading = async {
            reader.read_to_end(&mut received_replay_file).await.unwrap();
        };

        join! { run_replay, replay_reading, replay_writing };
        compare_bufs(example_replay_file, received_replay_file);
    }

    #[tokio::test]
    async fn test_replay_stops_accepting_connections() {
        setup_logging();
        tokio::time::pause();

        let mut mock_saver = InnerReplaySaver::faux();
        faux::when!(mock_saver.save_replay).then_do(|| ());
        let token = CancellationToken::new();
        let mut config = default_config();
        config.replay.time_with_zero_writers_to_end_replay_s = 2;
        let replay = Replay::new(1, token, Arc::new(config), Arc::new(mock_saver));

        let (mut c1, _r1, mut w1) = test_connection();
        let (mut c2, mut r2, w2) = test_connection();
        let (mut c3, _r3, w3) = test_connection();
        let (mut c4, mut r4, w4) = test_connection();
        c1.set_header(ConnectionHeader {
            type_: ConnectionType::WRITER,
            id: 1,
            name: "foo".into(),
        });
        c2.set_header(ConnectionHeader {
            type_: ConnectionType::READER,
            id: 1,
            name: "foo".into(),
        });
        c3.set_header(ConnectionHeader {
            type_: ConnectionType::WRITER,
            id: 1,
            name: "foo".into(),
        });
        c4.set_header(ConnectionHeader {
            type_: ConnectionType::READER,
            id: 1,
            name: "foo".into(),
        });

        let example_replay_file = get_file("example");
        let replay_is_over = std::cell::Cell::new(false);
        let replay_lifetime = async {
            replay.lifetime().await;
            replay_is_over.set(true);
        };

        // TIMELINE:
        // 0 - Writer (c1) arrives
        // 5 - Reader (c2) arrives
        // 7 - c1 writes all replay data
        // 9 - Replay stops accepting new connections
        // 15 - Writer (c3) arrives, is discarded
        // 25 - Reader (c4) arrives, is discarded
        // 35 - Reader (c2) ends
        let events = async {
            join! {
                async {
                    let res = replay.handle_connection(c1).await;
                    assert!(matches!(res, Ok(..)));
                },
                async {
                    sleep_s(2).await;
                    assert_eq!(replay.writer_connection_count.count(), 1);
                    assert_eq!(replay.reader_connection_count.count(), 0);
                },
                async {
                    sleep_s(5).await;
                    let res = replay.handle_connection(c2).await;
                    assert!(matches!(res, Ok(..)));
                },
                async {
                    sleep_s(6).await;
                    assert_eq!(replay.writer_connection_count.count(), 1);
                    assert_eq!(replay.reader_connection_count.count(), 1);
                },
                async {sleep_s(7).await; w1.write_all(&example_replay_file).await.unwrap(); drop(w1);},
                async {
                    sleep_s(10).await;
                    assert_eq!(replay.writer_connection_count.count(), 0);
                    assert_eq!(replay.reader_connection_count.count(), 1);
                },
                async {
                    sleep_s(15).await;
                    let res = replay.handle_connection(c3).await;
                    assert!(matches!(res.unwrap_err(), ConnectionError::CannotAssignToReplay));
                },
                async {
                    sleep_s(16).await;
                    assert_eq!(replay.writer_connection_count.count(), 0);
                    assert_eq!(replay.reader_connection_count.count(), 1);
                },
                async {
                    sleep_s(25).await;
                    let res = replay.handle_connection(c4).await;
                    assert!(matches!(res.unwrap_err(), ConnectionError::CannotAssignToReplay));
                },
                async {
                    sleep_s(26).await;
                    assert_eq!(replay.writer_connection_count.count(), 0);
                    assert_eq!(replay.reader_connection_count.count(), 1);
                    assert!(!replay_is_over.get());
                },
                async {
                    sleep_s(30).await;
                    let mut v = Vec::new();
                    r2.read_to_end(&mut v).await.unwrap();
                    let mut rejected = Vec::new();
                    r4.read_to_end(&mut rejected).await.unwrap();
                    assert!(rejected.is_empty());
                    drop(w2);
                    drop(w3);
                    drop(w4);

                    // FIXME https://github.com/tokio-rs/tokio/issues/3562
                    sleep_s(5).await;
                    assert_eq!(replay.writer_connection_count.count(), 0);
                    assert_eq!(replay.reader_connection_count.count(), 0);
                    assert!(replay_is_over.get());
                },
            }
        };

        join! {
            replay_lifetime,
            events,
        };
    }
}
