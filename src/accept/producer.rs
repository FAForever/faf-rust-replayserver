use async_std::net::{TcpListener, TcpStream};
use async_std::stream;
use stop_token::StopToken;
use futures::stream::StreamExt;

use crate::server::connection::Connection;

use super::ConnectionAcceptor;

pub struct ConnectionProducer {
    addr: String,
    acceptor: ConnectionAcceptor,
    shutdown_token: StopToken,
}

impl ConnectionProducer {
    pub fn new(
            addr: String,
            shutdown_token: StopToken,
            acceptor: ConnectionAcceptor,
            ) -> Self {
        ConnectionProducer {addr, acceptor, shutdown_token}
    }

    pub async fn accept(&self) -> () {
        let listener = TcpListener::bind(self.addr.clone());
        let incoming = listener.await.unwrap();     // TODO handle
        let incoming = incoming.incoming();
        let incoming= self.shutdown_token
            .stop_stream(incoming)
            .filter_map(ConnectionProducer::_filter_good);

        incoming
            .zip(stream::once(&self.acceptor).cycle())
            .for_each_concurrent(None, Self::send).await;
    }

    async fn _filter_good(item: Result<TcpStream, std::io::Error>) -> Option<TcpStream> {
        match item {
            Ok(v) => Some(v),
            Err(_) => None,     // TODO log
        }
    }

    async fn send(args: (TcpStream, &ConnectionAcceptor)) {
            let (stream, acceptor) = args;
            let conn = Connection::new(stream);  /* TODO use a builder */
            acceptor.accept(conn).await
    }
}
