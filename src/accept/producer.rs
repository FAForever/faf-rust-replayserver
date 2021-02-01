use crate::server::connection::Connection;
use async_stream::stream;
use futures::Stream;
use tokio::net::TcpListener;

pub struct ConnectionProducer {
    addr: String,
}

impl ConnectionProducer {
    pub fn new(addr: String) -> Self {
        Self { addr }
    }

    /* Returned stream never ends. */
    pub async fn connections(&self) -> impl Stream<Item = Connection> {
        let listener = TcpListener::bind(self.addr.clone()).await.unwrap();
        stream! {
            loop {
                match listener.accept().await {
                    Err(_) => (),    /* log? */
                    Ok((socket, _addr)) => {
                        /* Tight coupling. That's okay. */
                        yield Connection::new(socket);
                    }
                }
            }
        }
    }
}
