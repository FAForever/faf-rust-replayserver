use std::fs;
use std::io::prelude::*;
use std::net::TcpListener;
use std::net::TcpStream;

pub fn accept_connections() {
    // Listen for incoming TCP connections on localhost port 7878
    let listener = TcpListener::bind("127.0.0.1:7878").unwrap();

    // Block forever, handling each request that arrives at this IP address
    for stream in listener.incoming() {
        let stream = stream.unwrap();

        handle_connection(stream);
    }
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

fn handle_connection(mut stream: TcpStream) {
    // Read the first 1024 bytes of data from the stream
    let mut buffer = [0; 1024];
    stream.read(&mut buffer).unwrap();

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
    stream.write(response.as_bytes()).unwrap();
    stream.flush().unwrap();
}
