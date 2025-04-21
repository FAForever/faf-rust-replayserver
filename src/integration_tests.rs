#[cfg(test)]
mod test {
    // Three external pieces we test against:
    // * Replay save directory. Easy, we just make a temporary one.
    // * Ports. Relatively easy, we pass 0 and receive the actual port.
    // * Database. Hard, making a separate instance for every test is a bother.
    //   Just use one database and be careful not to step on other tests' toes.

    use std::{path::PathBuf, sync::Arc, time::Duration};

    use rand::Rng;
    use tokio::{io::{AsyncReadExt, AsyncWriteExt}, join, net::TcpStream, select};
    use tokio_util::sync::CancellationToken;

    use crate::{config::test::default_config, server::server::server_with_real_deps, util::test::{compare_bufs, get_file, setup_logging, sleep_ms}};

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test(flavor = "multi_thread")]
    async fn integration_test_server_single_empty_connection() {
        setup_logging();

        let replay_dir = tempfile::tempdir().unwrap();
        let mut config = default_config();
        config.server.port = Some(0);
        config.server.websocket_port = None;
        config.database.host = std::env::var("DB_HOST").unwrap();
        config.database.port = str::parse::<u16>(&std::env::var("DB_PORT").unwrap()).unwrap();
        config.storage.vault_path = replay_dir.path().to_str().unwrap().into();
        config.server.connection_accept_timeout_s = Duration::from_millis(10);

        let token = CancellationToken::new();
        let (server, _) = server_with_real_deps(Arc::new(config), token.child_token()).await;
        let mut ended_too_early = true;

        let wait = async {
            sleep_ms(100).await;
            ended_too_early = false;
            token.cancel();
            sleep_ms(100).await;
        };

        select! {
            _ = server.run() => (),
            _ = wait => panic!("Server should have quit after cancelling token"),
        }
        assert!(!ended_too_early);
    }

    async fn random_sleep() {
        tokio::time::sleep(Duration::from_millis(rand::rng().random_range(1..10))).await;
    }

    async fn tcp_writer(port: u16, header: &[u8], file: Vec<u8>) {
        let mut tcp = TcpStream::connect(format!("127.0.0.1:{}", port)).await.unwrap();
        random_sleep().await;
        tcp.write_all(header).await.unwrap();
        random_sleep().await;
        for data in file.chunks(1000) {
                tcp.write_all(data).await.unwrap();
                random_sleep().await;
        }
    }

    async fn tcp_reader(port: u16, header: &[u8]) -> Vec<u8> {
        let mut tcp = TcpStream::connect(format!("127.0.0.1:{}", port)).await.unwrap();
        // Wait for the writer to create a replay
        tokio::time::sleep(Duration::from_millis(30)).await;
        tcp.write_all(header).await.unwrap();
        random_sleep().await;

        let mut output: Vec<u8> = Default::default();
        tcp.read_to_end(&mut output).await.unwrap();
        output
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test(flavor = "multi_thread")]
    async fn integration_test_server_one_writer_one_reader() {
        // Game ID in db: 2000

        setup_logging();

        let replay_dir = tempfile::tempdir().unwrap();
        let mut config = default_config();
        config.server.port = Some(0);
        config.server.websocket_port = None;
        config.database.host = std::env::var("DB_HOST").unwrap();
        config.database.port = str::parse::<u16>(&std::env::var("DB_PORT").unwrap()).unwrap();
        config.storage.vault_path = replay_dir.path().to_str().unwrap().into();
        config.server.connection_accept_timeout_s = Duration::from_millis(50);

        let token = CancellationToken::new();
        let (server, port_info) = server_with_real_deps(Arc::new(config), token.child_token()).await;

        let sent_replay = get_file("example");
        let replay_writer = tcp_writer(port_info.tcp.unwrap(), b"P/2000/foo\0", sent_replay.clone());
        let replay_reader = tcp_reader(port_info.tcp.unwrap(), b"G/2000/foo\0");

        let exit_server = async move || {
            tokio::time::sleep(Duration::from_secs(1)).await;
            token.cancel();
        };

        let run = async move || {
            let (_, _, reader_data, _) = join! {
               server.run(),
               exit_server(),
               replay_reader,
               replay_writer,
            };
            reader_data
        };
        let reader_data = tokio::time::timeout(Duration::from_secs(2), run()).await.unwrap();
        compare_bufs(reader_data, sent_replay);

        let mut replay_path: PathBuf = replay_dir.path().to_owned();
        replay_path.push("0");
        replay_path.push("0");
        replay_path.push("0");
        replay_path.push("20");
        replay_path.push("2000.fafreplay");
        let saved_replay = std::fs::read(&replay_path).unwrap();
        // This file was copied from this test and manually verified to be correct.
        // Zstd is deterministic for the same data + params + version, let's rely on that.
        let reference_replay = get_file("integration/game_2000.fafreplay");
        compare_bufs(saved_replay, reference_replay);

        // TODO: verify stats saved to the database.
    }
}
