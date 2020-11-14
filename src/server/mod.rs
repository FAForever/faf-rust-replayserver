mod server;

pub async fn accept_connections() {
    server::accept_connections().await
}
