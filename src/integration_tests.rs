#[cfg(test)]
mod test {
    use std::{sync::Arc, time::Duration};

    use tokio::select;
    use tokio_util::sync::CancellationToken;

    use crate::{config::test::default_config, server::server::server_with_real_deps, util::test::{setup_logging, sleep_ms}};

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
}
