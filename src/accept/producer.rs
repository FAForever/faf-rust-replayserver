use crate::server::connection::Connection;
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
            Ok(s) => Some(Connection::new(s))
        }
    })
}
