use super::connection::Connection;
use crate::accept::header::read_initial_header;
use crate::database::database::Database;
use crate::replay::runner::ReplayRunner;
use crate::util::timeout::cancellable;
use crate::{accept::producer::tcp_listen, config::Settings, replay::save::InnerReplaySaver};
use crate::{metrics, replay::save::SavedReplayDirectory};
use futures::{stream::StreamExt, Stream};
use tokio_util::sync::CancellationToken;

struct Server<C: Stream<Item = Connection>> {
    config: Settings,
    shutdown_token: CancellationToken,
    connections: C,
    db: Database,
    dir: SavedReplayDirectory,
}

impl<C: Stream<Item = Connection>> Server<C> {
    fn new(
        config: Settings,
        shutdown_token: CancellationToken,
        connections: C,
        db: Database,
        dir: SavedReplayDirectory,
    ) -> Self {
        Self {
            config,
            shutdown_token,
            connections,
            db,
            dir,
        }
    }

    async fn run(self) {
        let saver = InnerReplaySaver::new(self.db, self.dir, &self.config);
        let runner = ReplayRunner::new(self.config.clone(), self.shutdown_token.clone(), saver);

        let initial_timeout = self.config.server.connection_accept_timeout_s;
        let accept_connections = self.connections.for_each_concurrent(None, |mut c| async {
            match read_initial_header(&mut c, initial_timeout).await {
                Err(e) => {
                    log::info!("Could not accept connection: {}", e);
                    metrics::inc_served_conns(Some(e));
                }
                Ok(_) => runner.dispatch_connection(c).await,
            }
        });

        match cancellable(accept_connections, &self.shutdown_token).await {
            Some(_) => log::warn!("Server stopped accepting connections for some reason!"),
            None => log::info!("Server shutting down"),
        }

        let _ = tokio::task::spawn_blocking(|| runner.shutdown()).await;
    }
}

async fn server_with_real_deps(
    config: Settings,
    shutdown_token: CancellationToken,
) -> Server<impl Stream<Item = Connection>> {
    let connections = tcp_listen(format!("0.0.0.0:{}", config.server.port)).await;
    let db = Database::new(&config.database);
    let dir = SavedReplayDirectory::new(config.storage.vault_path.as_ref());
    Server::new(config, shutdown_token, connections, db, dir)
}

pub async fn run_server(config: Settings, shutdown_token: CancellationToken) {
    server_with_real_deps(config, shutdown_token).await.run().await;
}

#[cfg(test)]
mod test {
    use crate::{
        config::test::default_config,
        database::database::test::mock_database,
        server::connection::test::test_connection,
        util::test::{get_file, setup_logging, sleep_s},
    };
    use async_stream::stream;
    use futures::future::join_all;
    use std::{
        rc::Rc,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
    };
    use tempfile::{tempdir, TempDir};
    use tokio::{fs::File, time::Duration};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        join, select,
    };

    use super::*;
    use crate::replay::save::directory::test::test_directory;
    use crate::replay::save::test::unpack_replay;
    use crate::util::test::compare_bufs;

    fn temp_replay_dir() -> (TempDir, SavedReplayDirectory) {
        let tmp_dir = tempdir().unwrap();
        let dir_str = tmp_dir.path().to_str().unwrap().into();
        let dir = SavedReplayDirectory::new(dir_str);
        (tmp_dir, dir)
    }

    #[tokio::test]
    async fn test_server_single_empty_connection() {
        setup_logging();
        tokio::time::pause();

        let (c, _reader, _writer) = test_connection();
        let mut conf = default_config();
        let db = mock_database();
        let token = CancellationToken::new();
        let replay_dir = test_directory();

        conf.server.connection_accept_timeout_s = Duration::from_secs(20);

        let server = Server::new(Arc::new(conf), token.clone(), stream! { yield c; }, db, replay_dir).run();
        let mut ended_too_early = true;

        let wait = async {
            sleep_s(19).await;
            ended_too_early = false;
            sleep_s(2).await;
        };

        select! {
            _ = server => (),
            _ = wait => panic!("Server should have quit after connection timeout"),
        }
        assert!(!ended_too_early);
        /* TODO check that nothing happened, maybe, somehow? */
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_one_writer_one_reader() {
        // We can't use tokio::time::pause, because worker threads have their own runtimes.
        // Even if we paused them for tests, we'd have to synchronize them manually with a
        // sender/receiver mess.
        //
        // Oh well, too bad. We can still test 90% of the important stuff by testing
        // Replay/Replays/Saver classes.
        setup_logging();

        let (c_read, mut reader, mut read_writer) = test_connection();
        let (c_write, _reader, mut writer) = test_connection();
        let mut conf = default_config();
        let db = mock_database();
        let token = CancellationToken::new();
        let (tmpdir, replay_dir) = temp_replay_dir(); // Use a real temp directory to verify path

        conf.replay.time_with_zero_writers_to_end_replay_s = Duration::from_secs(1);

        let conn_source = stream! {
            yield c_write;
            tokio::time::sleep(Duration::from_millis(100)).await;
            yield c_read;
        };
        let server = Server::new(Arc::new(conf), token.clone(), conn_source, db, replay_dir).run();

        let example_replay_file = get_file("example");
        let replay_writing = async {
            writer.write_all(b"P/2/foo\0").await.unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
            for data in example_replay_file.chunks(100) {
                writer.write_all(data).await.unwrap();
                tokio::time::sleep(Duration::from_millis(3)).await;
            }
            drop(writer);
        };
        let mut received_replay_file = Vec::<u8>::new();
        let replay_reading = async {
            read_writer.write_all(b"G/2/foo\0").await.unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
            reader.read_to_end(&mut received_replay_file).await.unwrap();
        };

        let server_thread = tokio::spawn(server);
        let (_, _, res) = join! {
            replay_reading,
            replay_writing,
            server_thread,
        };
        res.unwrap();

        let mut file_path = tmpdir.path().to_owned();
        file_path.push("0/0/0/0/2.fafreplay");
        let replay_file = File::open(file_path).await.unwrap();
        let (json, saved_replay) = unpack_replay(replay_file).await.unwrap();
        assert!(json.len() > 0);
        assert_eq!(json[0], b'{');
        assert_eq!(json[json.len() - 1], b'\n');
        compare_bufs(example_replay_file, saved_replay);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_ends_quickly() {
        setup_logging();

        let (c_read, mut reader, mut read_writer) = test_connection();
        let (c_write, _reader, mut writer) = test_connection();
        let (c_empty, _empty_reader, empty_writer) = test_connection();
        let mut conf = default_config();
        let db = mock_database();
        let token = CancellationToken::new();
        let replay_dir = test_directory();

        let server_ended = Arc::new(AtomicBool::new(false));

        conf.replay.time_with_zero_writers_to_end_replay_s = Duration::from_secs(1);

        let conn_source = stream! {
            yield c_write;
            yield c_empty;
            tokio::time::sleep(Duration::from_millis(10)).await;
            yield c_read;
        };
        let token_c = token.clone();
        let server_ended_c = server_ended.clone();
        let server = async move {
            Server::new(Arc::new(conf), token_c, conn_source, db, replay_dir)
                .run()
                .await;
            server_ended_c.store(true, Ordering::Relaxed);
        };

        let example_replay_file = get_file("example");
        let empty_replay = async {
            tokio::time::sleep(Duration::from_millis(500)).await;
            drop(empty_writer);
        };
        let replay_writing = async {
            writer.write_all(b"P/2/foo\0").await.unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
            writer.write_all(&example_replay_file[..500]).await.unwrap();
            tokio::time::sleep(Duration::from_millis(500)).await;
            drop(writer);
        };
        let mut received_replay_file = Vec::<u8>::new();
        let replay_reading = async {
            read_writer.write_all(b"G/2/foo\0").await.unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
            reader.read_to_end(&mut received_replay_file).await.unwrap();
        };

        let cancel_early = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            token.cancel();
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert!(server_ended.load(Ordering::Relaxed));
        };

        let server_thread = tokio::spawn(server);
        let (_, _, _, _, res) = join! {
            replay_reading,
            replay_writing,
            empty_replay,
            cancel_early,
            server_thread,
        };
        res.unwrap();
    }

    #[cfg_attr(not(feature = "bench"), ignore)]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_simple_benchmark() {
        // Can't use pause here either.
        const REPLAYS: usize = 500;
        const CONNS_SIDE_PER_REPLAY: usize = 5;
        let mut r_conns = Vec::new();
        let mut w_conns = Vec::new();
        let mut r_rw = Vec::new();
        let mut w_readers = Vec::new();
        let mut w_writers = Vec::new();

        for _ in 0..(REPLAYS * CONNS_SIDE_PER_REPLAY) {
            let (c, r, w) = test_connection();
            r_conns.push(c);
            r_rw.push((r, w));
            let (c, r, w) = test_connection();
            w_conns.push(c);
            w_readers.push(r);
            w_writers.push(w);
        }

        let mut conf = default_config();
        let db = mock_database();
        let token = CancellationToken::new();
        let replay_dir = test_directory();
        let example_replay_file = Rc::new(get_file("example"));

        conf.replay.time_with_zero_writers_to_end_replay_s = Duration::from_secs(1);
        conf.replay.delay_s = Duration::from_secs(1);
        conf.replay.update_interval_s = Duration::from_millis(100);

        // faux panic when accessed from multiple threads despite being sync.
        // We don't need multiple threads for this test anyway.
        conf.server.worker_threads = 1;

        let conn_source = stream! {
            for c in w_conns.into_iter() {
                yield c;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            for c in r_conns.into_iter() {
                yield c;
            }
        };
        let server = Server::new(Arc::new(conf), token.clone(), conn_source, db, replay_dir).run();

        let replay_writing = |mut w: tokio::io::DuplexStream, i: usize| async move {
            w.write_all(format!("P/{}/foo\0", i).into_bytes().as_ref())
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
            for data in example_replay_file.chunks(100) {
                w.write_all(data).await.unwrap();
                tokio::time::sleep(Duration::from_millis(3)).await;
            }
            drop(w);
        };
        let replay_reading = |mut r: tokio::io::DuplexStream, mut w: tokio::io::DuplexStream, i: usize| async move {
            let mut buf: Box<[u8]> = Box::new([0; 256]);
            w.write_all(format!("G/{}/foo\0", i).into_bytes().as_ref())
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
            loop {
                let res = r.read(&mut *buf).await.unwrap();
                if res == 0 {
                    break;
                }
            }
        };

        let write_functions = w_writers.into_iter().enumerate().map(|e| {
            let (i, w) = e;
            replay_writing.clone()(w, (i / CONNS_SIDE_PER_REPLAY) + 1)
        });
        let read_functions = r_rw.into_iter().enumerate().map(|e| {
            let (i, (r, w)) = e;
            replay_reading(r, w, (i / CONNS_SIDE_PER_REPLAY) + 1)
        });
        let server_thread = tokio::spawn(server);
        let (_, _, res) = join! {
            join_all(write_functions),
            join_all(read_functions),
            server_thread,
        };
        res.unwrap();
    }
}
