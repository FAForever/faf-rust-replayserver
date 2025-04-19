use crate::server::{connection::Connection, websocket_stream::make_split_websocket_from_tcp};
use futures::{Stream, StreamExt};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

pub async fn tcp_listen(addr: String) -> impl Stream<Item = Connection> {
    let listener = TcpListener::bind(addr).await.unwrap();
    TcpListenerStream::new(listener).filter_map(|c| async {
        match c {
            Err(e) => {
                log::info!("Failed to accept connection: {}", e);
                None
            }
            Ok(s) => Some(Connection::new(s)),
        }
    })
}

pub async fn websocket_listen(addr: String) -> impl Stream<Item = Connection> {
    let listener = TcpListener::bind(addr).await.unwrap();
    TcpListenerStream::new(listener).filter_map(|c| async {
        match c {
            Err(e) => {
                log::info!("Failed to accept connection: {}", e);
                None
            }
            Ok(s) => {
                match make_split_websocket_from_tcp(s).await {
                    Err(e) => {
                        log::info!("Failed to create websocket: {}", e);
                        None
                    }
                    Ok((r, w)) => Some(Connection::new_from(r, w))
                }
            }
        }
    })
}
