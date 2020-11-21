use crate::server::workers::{ReplayWorkerThread,dummy_work};
use crate::config::Config;
use async_std::net::{TcpListener, TcpStream};
use async_std::stream;
use futures::stream::StreamExt;
use stop_token::StopToken;
use async_channel::Sender;
use std::cell::RefCell;
use log::debug;

use crate::server::connection::Connection;

type ChannelRef = RefCell<Vec<Sender<Connection>>>;

pub struct Server
{
    replay_workers: Vec<ReplayWorkerThread>,
    send_channels: ChannelRef,
    port: String,
}

struct SelectArgs
{
    stream: TcpStream,
    token: StopToken,
    channels: ChannelRef,
}

impl Server {
    pub fn new(config: &Config) -> Self
    {
        let mut threads = Vec::new();
        let mut channels = Vec::new();
        for _ in 0..config.worker_threads {
            let worker = ReplayWorkerThread::new(dummy_work);
            channels.push(worker.get_channel());
            threads.push(worker);
        }
        Server {
            replay_workers: threads,
            send_channels: RefCell::new(channels),
            port: config.port.clone(),
        }
    }

    /* TODO move accepting elsewhere */
    pub async fn accept_until(&self, token: StopToken) {
        let addr = format!("localhost:{}", self.port);
        Server::_handle_conn_stream(addr, token, self.send_channels.clone()).await;
    }

    async fn _filter_good(item: Result<TcpStream, std::io::Error>) -> Option<TcpStream> {
        match item {
            Ok(v) => Some(v),
            Err(_) => None,     // TODO log
        }
    }

    async fn _handle_conn_stream(
            addr: String,
            token: StopToken,
            channels: ChannelRef) {
        let listener = TcpListener::bind(addr).await.unwrap();    /* TODO log */
        let incoming = listener.incoming();
        let bound_incoming = token.stop_stream(incoming);
        let only_good = bound_incoming.filter_map(Server::_filter_good);

        // Some mess to avoid async closures
        let tokens = stream::once(token).cycle();
        let channels = stream::once(channels).cycle();
        let inc_plus_token = only_good.zip(tokens).zip(channels).map(|t| {
            let ((stream, token), channels) = t;
            SelectArgs {stream, token, channels}
        });
        inc_plus_token.for_each_concurrent(None, Server::handle_connection).await;
        debug!("Connection stream closed.");
    }

    async fn handle_connection(args: SelectArgs)
    {
        let SelectArgs {stream, token, channels} = args;
        drop(token);
        let connection = Connection::new(stream);
        // Safe since we're in the same thread. TODO document.
        let target_thread = &channels.borrow()[0];
        target_thread.send(connection).await.unwrap();
    }

    pub fn shutdown(&mut self) {
        for thread in self.replay_workers.iter_mut() {
            thread.shutdown();
        }
        /* join happens via thread destructors */
    }
}
