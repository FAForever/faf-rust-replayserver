use super::connection::Connection;
use crate::accept::ConnectionAcceptor;
use crate::config::Settings;
use crate::{accept::ConnectionProducer, replay::Replays, worker_threads::ReplayThreadPool};
use futures::stream::StreamExt;
use log::debug;
use tokio::{select, sync::mpsc::Receiver};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

pub struct Server {
    acceptor: ConnectionAcceptor,
    producer: ConnectionProducer,
    shutdown_token: CancellationToken,
}

pub fn worker_thread_fn(
    streams: Receiver<Connection>,
    shutdown_token: CancellationToken,
    config: Settings,
) {
    let local_loop = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    local_loop.block_on(worker_thread_work(streams, shutdown_token, config));
}

async fn worker_thread_work(
    streams: Receiver<Connection>,
    shutdown_token: CancellationToken,
    config: Settings,
) {
    let mut replays = Replays::build(shutdown_token, config);
    let wrapper = ReceiverStream::new(streams);
    replays.handle_connections_and_replays(wrapper).await;
}

impl Server {
    pub fn new(
        config: Settings,
        producer: ConnectionProducer,
        shutdown_token: CancellationToken,
    ) -> Self {
        let c = config.clone();
        let t = shutdown_token.clone();
        let work = Box::new(move |s| worker_thread_fn(s, t.clone(), c.clone()));
        let thread_pool = ReplayThreadPool::new(move || work.clone(), config.server.worker_threads);
        let acceptor = ConnectionAcceptor::build(thread_pool, &config);
        Self {
            acceptor,
            producer,
            shutdown_token,
        }
    }

    pub async fn accept(&self) {
        let connections = self.producer.connections().await;
        let acceptor = &self.acceptor;
        let work = connections.for_each_concurrent(None, |c| async move {
            acceptor.accept(c).await;
        });
        select! {
            _ = work => { debug!("Server stopped accepting connections for some reason!") }
            _ = self.shutdown_token.cancelled() => { debug!("Server shutting down") }
        }
    }
}
