use super::connection::Connection;
use crate::metrics;
use crate::util::timeout::cancellable;
use crate::worker_threads::ReplayThreadPool;
use crate::{
    accept::producer::tcp_listen, config::Settings, replay::save::InnerReplaySaver,
    worker_threads::ReplayThreadContext,
};
use crate::{accept::ConnectionAcceptor, database::database::Database, replay::save::ReplaySaver};
use futures::{stream::StreamExt, Stream};
use log::{debug, info};
use tokio_util::sync::CancellationToken;

async fn real_server_deps(config: Settings) -> (impl Stream<Item = Connection>, Database) {
    let connections = tcp_listen(format!("127.0.0.1:{}", config.server.port)).await;
    let database = Database::new(&config.database);
    (connections, database)
}

fn server_thread_pool(
    config: Settings,
    shutdown_token: CancellationToken,
    saver: ReplaySaver,
) -> ReplayThreadPool {
    let thread_count = config.server.worker_threads;
    let context = ReplayThreadContext::new(config, shutdown_token, saver);
    ReplayThreadPool::from_context(context, thread_count)
}

pub async fn run_server_with_deps(
    config: Settings,
    shutdown_token: CancellationToken,
    connections: impl Stream<Item = Connection>,
    database: Database,
) {
    let saver = InnerReplaySaver::new(database, config.clone());
    let thread_pool = server_thread_pool(config.clone(), shutdown_token.clone(), saver);
    let acceptor = ConnectionAcceptor::new(config);

    let accept_connections = connections.for_each_concurrent(None, |mut c| async {
        if let Err(e) = acceptor.accept(&mut c).await {
            info!("Could not accept connection: {}", e);
            metrics::inc_served_conns::<()>(&Err(e));
            return;
        }
        thread_pool.assign_connection(c).await;
    });

    match cancellable(accept_connections, &shutdown_token).await {
        Some(_) => debug!("Server stopped accepting connections for some reason!"),
        None => debug!("Server shutting down"),
    }

    thread_pool.join();
}

pub async fn run_server(config: Settings, shutdown_token: CancellationToken) {
    let (producer, database) = real_server_deps(config.clone()).await;
    run_server_with_deps(config, shutdown_token, producer, database).await
}

#[cfg(test)]
mod test {
    use crate::{
        config::test::default_config,
        database::database::test::mock_database,
        server::connection::test::test_connection,
        util::test::{get_file, setup_logging},
    };
    use async_stream::stream;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use tempfile::tempdir;
    use tokio::{fs::File, time::Duration};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        join, select,
    };

    use super::*;
    use crate::replay::save::test::unpack_replay;
    use crate::util::test::compare_bufs;

    #[tokio::test]
    async fn test_server_single_empty_connection() {
        setup_logging();
        tokio::time::pause();

        let (c, _reader, _writer) = test_connection();
        let mut conf = default_config();
        let db = mock_database();
        let token = CancellationToken::new();
        let tmp_dir = tempdir().unwrap();

        conf.server.connection_accept_timeout_s = 20;
        conf.storage.vault_path = tmp_dir.path().to_str().unwrap().into();

        let server = run_server_with_deps(Arc::new(conf), token.clone(), stream! { yield c; }, db);
        let mut ended_too_early = true;

        let wait = async {
            tokio::time::sleep(Duration::from_secs(19)).await;
            ended_too_early = false;
            tokio::time::sleep(Duration::from_secs(2)).await;
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
        let tmp_dir = tempdir().unwrap();

        conf.storage.vault_path = tmp_dir.path().to_str().unwrap().into();
        conf.replay.time_with_zero_writers_to_end_replay_s = 1;

        let conn_source = stream! {
            yield c_write;
            tokio::time::sleep(Duration::from_millis(100)).await;
            yield c_read;
        };
        let server = run_server_with_deps(Arc::new(conf), token.clone(), conn_source, db);

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

        let mut file_path = tmp_dir.path().to_owned();
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
        let tmp_dir = tempdir().unwrap();

        let server_ended = Arc::new(AtomicBool::new(false));

        conf.storage.vault_path = tmp_dir.path().to_str().unwrap().into();
        conf.replay.time_with_zero_writers_to_end_replay_s = 1;

        let conn_source = stream! {
            yield c_write;
            yield c_empty;
            tokio::time::sleep(Duration::from_millis(10)).await;
            yield c_read;
        };
        let token_c = token.clone();
        let server_ended_c = server_ended.clone();
        let server = async move {
            run_server_with_deps(Arc::new(conf), token_c, conn_source, db).await;
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
}
