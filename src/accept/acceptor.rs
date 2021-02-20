use std::time::Duration;

use tokio::select;

use super::header::ConnectionHeaderReader;
use crate::server::connection::Connection;
use crate::{
    config::Settings, error::ConnResult, error::ConnectionError, worker_threads::ReplayThreadPool,
};

pub struct ConnectionAcceptor {
    header_reader: ConnectionHeaderReader,
    thread_pool: ReplayThreadPool,
    connection_accept_timeout: Duration,
}

impl ConnectionAcceptor {
    pub fn new(
        header_reader: ConnectionHeaderReader,
        thread_pool: ReplayThreadPool,
        connection_accept_timeout: Duration,
    ) -> Self {
        ConnectionAcceptor {
            header_reader,
            thread_pool,
            connection_accept_timeout,
        }
    }

    pub fn build(thread_pool: ReplayThreadPool, config: Settings) -> Self {
        let header_reader = ConnectionHeaderReader::new();
        let connection_accept_timeout =
            Duration::from_secs(config.server.connection_accept_timeout_s);
        ConnectionAcceptor::new(header_reader, thread_pool, connection_accept_timeout)
    }

    /* Cancellable. */
    async fn do_accept(&self, mut c: Connection) -> ConnResult<()> {
        (select! {
            res = self.header_reader.read_and_set_connection_header(&mut c) => res,
            _ = tokio::time::sleep(self.connection_accept_timeout) => Err(ConnectionError::bad_data("Timed out while accepting replay")),
        })?;
        self.thread_pool.assign_connection(c).await;
        Ok(())
    }

    pub async fn accept(&self, c: Connection) {
        if let Err(e) = self.do_accept(c).await {
            e.log(None.into());
        }
    }
}
