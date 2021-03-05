use tokio::time::Duration;

use crate::{config::Settings, error::ConnResult, error::bad_data};
use crate::{server::connection::Connection, util::timeout::timeout};

use super::header::header_reader::read_and_set_connection_header;

pub struct ConnectionAcceptor {
    timeout: Duration,
}

impl ConnectionAcceptor {
    pub fn new(config: Settings) -> Self {
        let timeout = Duration::from_secs(config.server.connection_accept_timeout_s);
        Self { timeout }
    }

    /* Cancellable. */
    pub async fn accept(&self, mut c: &mut Connection) -> ConnResult<()> {
        match timeout(read_and_set_connection_header(&mut c), self.timeout).await {
            Some(res) => res,
            None => Err(bad_data("Timed out while accepting connection")),
        }
    }
}
