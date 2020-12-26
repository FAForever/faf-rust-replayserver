use crate::accept::ConnectionProducer;
use crate::accept::ConnectionAcceptor;
use crate::config::Config;
use super::{workers::ReplayThreadPool, connection::Connection};
use super::workers::dummy_work;
use log::debug;
use tokio::select;
use tokio_util::sync::CancellationToken;
use futures::stream::StreamExt;

pub struct Server
{
    acceptor: ConnectionAcceptor,
    producer: ConnectionProducer,
    shutdown_token: CancellationToken,
}

impl Server {
    pub fn new(config: &Config,
               producer: ConnectionProducer,
               shutdown_token: CancellationToken) -> Self {
        let thread_pool = ReplayThreadPool::new(dummy_work, config.worker_threads, shutdown_token.clone());
        let acceptor = ConnectionAcceptor::build(thread_pool);
        Self { acceptor, producer, shutdown_token }
    }

    pub async fn accept(&self) {
        let connections = self.producer.connections().await;
        let work = connections.map(|c| {(&self.acceptor, c)})
            .for_each_concurrent(None, Self::accept_one);
        select! {
            _ = work => { unreachable!() }
            _ = self.shutdown_token.cancelled() => { debug!("Server shutting down") }
        }
    }

    /* Kludge, async closures not stable yet */
    async fn accept_one(args: (&ConnectionAcceptor, Connection)) -> () {
        let (a, c) = args;
        a.accept(c).await;
    }
}
