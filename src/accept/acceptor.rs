use stop_token::StopToken;
use async_trait::async_trait;
use crate::error::ConnResult;
use crate::server::connection::Connection;
use crate::server::workers::ReplayThreadPool;
use crate::util::Consumer;
use super::header::ConnectionHeaderReader;

pub struct ConnectionAcceptor {
    header_reader: ConnectionHeaderReader,
    thread_pool: ReplayThreadPool,
}

impl ConnectionAcceptor {
    pub fn new(header_reader: ConnectionHeaderReader,
               thread_pool: ReplayThreadPool) -> Self {
        ConnectionAcceptor { header_reader, thread_pool}
    }

    pub fn build(thread_pool: ReplayThreadPool,
                 stop_token: StopToken) -> Self {
        let header_reader = ConnectionHeaderReader::new(stop_token);
        ConnectionAcceptor::new(header_reader, thread_pool)
    }

    async fn do_accept(&self, mut c: Connection) -> ConnResult<()> {
        self.header_reader.read_and_set_connection_header(&mut c).await?;
        self.thread_pool.assign_connection(c).await;
        Ok(())
    }

    pub async fn accept(&self, c: Connection) {
        if let Err(e) = self.do_accept(c).await {
            /* The only things acceptor does are reading the header and dispatching to thread.  If
             * we failed to read the header, then we don't have it. If we succeeded, then dispatch
             * can't fail. Therefore we never have a header to log.
             */
            e.log(None.into());
        }
    }
}

#[async_trait(?Send)]
impl Consumer<Connection, ()> for ConnectionAcceptor {
    async fn consume(&self, c: Connection) {
        self.accept(c).await;
    }
}
