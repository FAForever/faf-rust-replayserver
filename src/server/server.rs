use async_std::prelude::*;
use futures::stream::StreamExt;
use async_std::net::TcpListener;
use async_std::net::TcpStream;
use async_std::io::Result;

pub async fn accept_connections() {
    // Listen for incoming TCP connections on localhost port 7878
    let listener = TcpListener::bind("127.0.0.1:7878").await.unwrap();

    async fn do_handle(s: Result<TcpStream>) {
        let s = s.unwrap();
        handle_connection(s).await;
    }
    // Block forever, handling each request that arrives at this IP address
    listener
        .incoming()
        .for_each_concurrent(None, do_handle).await;
}

macro_rules! page_fmt { () => (r#"
{}
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <title>Hello!</title>
  </head>
  <body>
    <h1>Hello!</h1>
    <p>{}</p>
  </body>
</html>
    "#)}

async fn handle_connection(mut stream: TcpStream) {
    // Read the first 1024 bytes of data from the stream
    let mut buffer = [0; 1024];
    stream.read(&mut buffer).await.unwrap();

    let get = b"GET / HTTP/1.1\r\n";

    // Respond with greetings or a 404,
    // depending on the data in the request
    let (status_line, contents) = if buffer.starts_with(get) {
        ("HTTP/1.1 200 OK\r\n\r\n", "Hello!")
    } else {
        ("HTTP/1.1 404 NOT FOUND\r\n\r\n", "Not found!")
    };
    // Write response back to the stream,
    // and flush the stream to ensure the response is sent back to the client
    let response = format!(page_fmt!(), status_line, contents);
    stream.write(response.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
}
