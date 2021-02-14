use std::pin::Pin;

use crate::server::connection::Connection;
use async_stream::stream;
use futures::Stream;
use tokio::net::TcpListener;

pub type ConnectionStream = Pin<Box<dyn Stream<Item = Connection>>>;

#[cfg_attr(test, faux::create)]
pub struct ConnectionProducer {
    addr: String
}

#[cfg_attr(test, faux::methods)]
impl ConnectionProducer {
    pub fn new(addr: String) -> Self {
        Self { addr }
    }

    pub fn listen(&self) -> ConnectionStream {
        let addr = self.addr.clone();
        let stream = stream! {
            let listener = TcpListener::bind(addr).await.unwrap();
            loop {
                match listener.accept().await {
                    Err(_) => (),    /* log? */
                    Ok((socket, _addr)) => {
                        /* Tight coupling. That's okay. */
                        yield Connection::new(socket);
                    }
                }
            }
        };
        Box::pin(stream)
    }
}
