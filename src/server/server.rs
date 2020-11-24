use crate::server::workers::{ReplayWorkerThread,dummy_work};
use crate::config::Config;
use async_std::sync::{Sender, Receiver};
use std::cell::RefCell;

use crate::server::connection::Connection;
use crate::accept::ConnectionAcceptor;

type ChannelRef = RefCell<Vec<Sender<Connection>>>;

pub struct Server
{
    replay_workers: Vec<ReplayWorkerThread>,
    acceptor: ConnectionAcceptor,
    acceptor_channel: Receiver<Connection>,
    send_channels: ChannelRef,
}

impl Server {
    pub fn new(config: &Config,
               acceptor: ConnectionAcceptor,
               acceptor_channel: Receiver<Connection>) -> Self
    {
        let mut replay_workers = Vec::new();
        let mut channels = Vec::new();
        for _ in 0..config.worker_threads {
            let worker = ReplayWorkerThread::new(dummy_work);
            channels.push(worker.get_channel());
            replay_workers.push(worker);
        }
        Server {
            replay_workers,
            send_channels: RefCell::new(channels),
            acceptor,
            acceptor_channel,
        }
    }

    pub async fn accept(&self) {
        return self.acceptor.accept().await;
    }

    pub fn shutdown(&mut self) {
        for thread in self.replay_workers.iter_mut() {
            thread.shutdown();
        }
        /* join happens via thread destructors */
    }
}
