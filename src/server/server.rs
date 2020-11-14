use async_channel::Receiver;
use async_std::io::Result;
use async_std::net::TcpListener;
use async_std::net::TcpStream;
use async_std::prelude::*;
use crate::server::workers::WorkerThread;
use futures::executor::LocalPool;
use futures::stream::StreamExt;
use futures::task::SpawnExt;
use std::cell::RefCell;

pub async fn accept_connections(workers: Vec<WorkerThread<TcpStream>>) {
    // Listen for incoming TCP connections on localhost port 7878
    let listener = TcpListener::bind("127.0.0.1:7878").await.unwrap();
    let nonce : RefCell<usize> = RefCell::new(0);

    async fn send_to_worker(s: Result<TcpStream>, workers: &Vec<WorkerThread<TcpStream>>, nonce: &RefCell<usize>) {
        let s = s.unwrap();
        let mut borrowed = nonce.borrow_mut();
        let count = *borrowed;
        *borrowed += 1;
        *borrowed %= workers.len();
        let channel = &workers[count].channel;
        channel.send(s).await.unwrap();
    }
    listener
        .incoming()
        .for_each_concurrent(None, |s| {send_to_worker(s, &workers, &nonce)}).await;
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

pub fn thread_accept_connections(streams: Receiver<TcpStream>)
{
    let mut pool = LocalPool::new();
    pool.spawner().spawn(thread_handle_concurrent_connections(streams)).unwrap();
    pool.run()
}

async fn thread_handle_concurrent_connections(streams: Receiver<TcpStream>)
{
    streams.for_each_concurrent(None, handle_connection).await;
}


async fn handle_connection(mut stream: TcpStream) {
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
