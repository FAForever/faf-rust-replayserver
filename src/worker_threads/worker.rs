use std::thread;
use std::thread::JoinHandle;
use tokio::sync::mpsc::{Sender, channel, Receiver};
use tokio_util::sync::CancellationToken;

use crate::server::connection::Connection;

pub type ThreadFn = fn(Receiver<Connection>, CancellationToken) -> ();

pub struct ReplayWorkerThread
{
    handle: Option<JoinHandle<()>>,
    channel: Sender<Connection>,
}

impl ReplayWorkerThread {
    pub fn new(work: ThreadFn, shutdown_token: CancellationToken) -> Self {
        let (s, r) = channel(1);
        let handle = thread::spawn(move || {
            work(r, shutdown_token)
        });
        ReplayWorkerThread {
            handle: Some(handle),
            channel: s,
        }
    }
    pub async fn dispatch(&self, c: Connection) {
        match self.channel.send(c).await {
            Ok(a) => a,
            _ => panic!("Could not dispatch a connection to a thread. Did it die?"),
        }
    }
}

impl Drop for ReplayWorkerThread {
    fn drop(&mut self) {
        self.handle.take().unwrap().join().unwrap();    /* TODO log */
    }
}
