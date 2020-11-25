use async_std::net::{TcpListener, TcpStream};
use async_std::stream;
use async_std::sync::{channel, Sender, Receiver};
use stop_token::StopToken;
use futures::stream::StreamExt;

use crate::server::connection::Connection;
use crate::error::ConnResult;

use super::header::{read_connection_info, ConnectionInfo};

pub struct ConnectionAcceptor {
    addr: String,
    conn_sender: Sender<Connection>,
    shutdown_token: StopToken,
}

impl ConnectionAcceptor {
    pub fn new(
            addr: String,
            shutdown_token: &StopToken,
            ) -> (Self, Receiver<Connection>) {
        let (conn_sender, r) =channel(1);
        let shutdown_token = shutdown_token.clone();
        (ConnectionAcceptor {addr, conn_sender, shutdown_token},  r)
    }

    pub async fn accept(&self) -> () {
        let listener = TcpListener::bind(self.addr.clone());    /* TODO log */
        let incoming = listener.await.unwrap();
        let incoming = incoming.incoming();    /* TODO log */
        let incoming= self.shutdown_token
            .stop_stream(incoming)
            .filter_map(ConnectionAcceptor::_filter_good);

        incoming
            .zip(stream::once(&self.conn_sender).cycle())
            .for_each_concurrent(None, Self::send).await;
    }

    async fn _filter_good(item: Result<TcpStream, std::io::Error>) -> Option<TcpStream> {
        match item {
            Ok(v) => Some(v),
            Err(_) => None,     // TODO log
        }
    }

    async fn send(args: (TcpStream, &Sender<Connection>)) {
            let (stream, conn_sender) = args;
            let mut conn = Connection::new(stream);
            let data: ConnectionInfo = read_connection_info(&mut conn).await.unwrap();
            conn_sender.send(conn).await;
    }
}
