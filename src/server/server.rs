use super::connection::Connection;
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

fn real_server_deps(config: Settings) -> (impl Stream<Item = Connection>, Database) {
    let connections = tcp_listen(format!("localhost:{}", config.server.port));
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
            info!("{}", e);
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
    let (producer, database) = real_server_deps(config.clone());
    run_server_with_deps(config, shutdown_token, producer, database).await
}

#[cfg(test)]
mod test {
    use std::{sync::Arc, time::Duration};

    use crate::{
        config::test::default_config, database::database::test::mock_database,
        server::connection::test::test_connection,
    };
    use async_stream::stream;
    use tempfile::tempdir;
    use tokio::select;

    use super::*;

    #[tokio::test]
    async fn single_empty_connection() {
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
}
