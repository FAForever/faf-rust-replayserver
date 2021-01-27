use std::thread;
use std::thread::JoinHandle;
use tokio::sync::mpsc::{Sender, channel, Receiver};

use crate::server::connection::Connection;

pub type ThreadFn = Box<dyn Fn(Receiver<Connection>) -> () + Send>;

pub struct ReplayWorkerThread
{
    handle: Option<JoinHandle<()>>,
    channel: Sender<Connection>,
}

impl ReplayWorkerThread {
    pub fn new(work: ThreadFn) -> Self {
        let (s, r) = channel(1);
        let handle = thread::spawn(move || { work(r) });
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
    // This waits until the worker thread joins.
    // FIXME is it a good idea to do this in Drop? We don't want to panic here.
    fn drop(&mut self) {
        self.handle.take().unwrap().join().unwrap();
    }
}
