use std::time::Duration;

use tokio::select;

use super::header::ConnectionHeaderReader;
use crate::server::connection::Connection;
use crate::{
    config::Settings, error::ConnResult, error::ConnectionError,
};

pub struct ConnectionAcceptor {
    header_reader: ConnectionHeaderReader,
    connection_accept_timeout: Duration,
}

impl ConnectionAcceptor {
    pub fn new(
        header_reader: ConnectionHeaderReader,
        connection_accept_timeout: Duration,
    ) -> Self {
        ConnectionAcceptor {
            header_reader,
            connection_accept_timeout,
        }
    }

    pub fn build(config: Settings) -> Self {
        let header_reader = ConnectionHeaderReader::new();
        let connection_accept_timeout =
            Duration::from_secs(config.server.connection_accept_timeout_s);
        ConnectionAcceptor::new(header_reader, connection_accept_timeout)
    }

    /* Cancellable. */
    pub async fn accept(&self, mut c: &mut Connection) -> ConnResult<()> {
        select! {
            res = self.header_reader.read_and_set_connection_header(&mut c) => res,
            _ = tokio::time::sleep(self.connection_accept_timeout) => Err(ConnectionError::bad_data("Timed out while accepting replay")),
        }
    }
}
