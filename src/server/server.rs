use crate::{accept::ConnectionProducer, replay::Replays, worker_threads::ReplayThreadPool};
use crate::accept::ConnectionAcceptor;
use crate::config::Settings;
use super::connection::Connection;
use log::debug;
use tokio::{select, sync::mpsc::Receiver};
use tokio_util::sync::CancellationToken;
use futures::stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

pub struct Server
{
    acceptor: ConnectionAcceptor,
    producer: ConnectionProducer,
    shutdown_token: CancellationToken,
}

pub fn worker_thread_fn(streams: Receiver<Connection>, shutdown_token: CancellationToken)
{
    let local_loop = tokio::runtime::Builder::new_current_thread().build().unwrap();
    local_loop.block_on(worker_thread_work(streams, shutdown_token));
}

async fn worker_thread_work(streams: Receiver<Connection>, shutdown_token: CancellationToken)
{
    let mut replays = Replays::build(shutdown_token);
    let wrapper = ReceiverStream::new(streams);
    replays.handle_connections_and_replays(wrapper).await;
}

impl Server {
    pub fn new(config: &Settings,
               producer: ConnectionProducer,
               shutdown_token: CancellationToken) -> Self {
        let thread_pool = ReplayThreadPool::new(worker_thread_fn, config.server.worker_threads, shutdown_token.clone());
        let acceptor = ConnectionAcceptor::build(thread_pool);
        Self { acceptor, producer, shutdown_token }
    }

    pub async fn accept(&self) {
        let connections = self.producer.connections().await;
        let acceptor = &self.acceptor;
        let work = connections.for_each_concurrent(None, |c| async move {
            acceptor.accept(c).await;
        });
        select! {
            _ = work => { unreachable!() }
            _ = self.shutdown_token.cancelled() => { debug!("Server shutting down") }
        }
    }
}
