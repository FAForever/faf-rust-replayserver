use std::pin::Pin;

use crate::server::connection::Connection;
use async_stream::stream;
use futures::Stream;
use tokio::net::TcpListener;

pub type ConnectionStream = Pin<Box<dyn Stream<Item = Connection>>>;

pub fn tcp_listen(addr: String) -> impl Stream<Item = Connection> {
    stream! {
        let listener = TcpListener::bind(addr).await.unwrap();
        loop {
            match listener.accept().await {
                Err(_) => (),    /* FIXME this should end listening, right? Log */
                Ok((socket, _addr)) => {
                    /* Tight coupling. That's okay. */
                    yield Connection::new(socket);
                }
            }
        }
    }
}
