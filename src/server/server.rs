use stop_token::StopToken;
use crate::accept::ConnectionProducer;
use crate::accept::ConnectionAcceptor;
use crate::config::Config;
use super::workers::ReplayThreadPool;
use super::workers::dummy_work;

pub struct Server
{
    producer: ConnectionProducer,
}

impl Server {
    pub fn new(config: &Config,
               producer: impl Fn(ConnectionAcceptor) -> ConnectionProducer,
               stop_token: StopToken) -> Self {
        let thread_pool = ReplayThreadPool::new(dummy_work, config.worker_threads, stop_token.clone());
        let acceptor = ConnectionAcceptor::build(thread_pool, stop_token);
        let producer = producer(acceptor);
        Self { producer }
    }

    pub async fn accept(&self) {
        return self.producer.accept().await;
    }
}
