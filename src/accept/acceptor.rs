use crate::error::ConnResult;
use crate::server::connection::Connection;
use crate::server::workers::ReplayThreadPool;
use super::header::ConnectionHeaderReader;

pub struct ConnectionAcceptor {
    header_reader: ConnectionHeaderReader,
    thread_pool: ReplayThreadPool,
}

impl ConnectionAcceptor {
    pub fn new(header_reader: ConnectionHeaderReader,
               thread_pool: ReplayThreadPool) -> Self {
        ConnectionAcceptor { header_reader, thread_pool }
    }

    pub fn build(thread_pool: ReplayThreadPool) -> Self {
        let header_reader = ConnectionHeaderReader::new();
        ConnectionAcceptor::new(header_reader, thread_pool)
    }

    /* Cancellable. */
    async fn do_accept(&self, mut c: Connection) -> ConnResult<()> {
        self.header_reader.read_and_set_connection_header(&mut c).await?;
        self.thread_pool.assign_connection(c).await;
        Ok(())
    }

    pub async fn accept(&self, c: Connection) {
        if let Err(e) = self.do_accept(c).await {
            e.log(None.into());
        }
    }
}
