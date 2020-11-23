use async_std::sync::{channel, Sender, Receiver};
use std::thread;
use std::thread::JoinHandle;
use crate::server::connection::Connection;
use stop_token::StopToken;
use futures::{
    executor::LocalPool,
    stream::StreamExt,
    task::SpawnExt,
    future::FutureExt,
    pin_mut,
    select,
};
use async_std::task::sleep;
use std::option::Option;
use std::time::Duration;
use stop_token::StopSource;

pub type ThreadFn = fn(Receiver<Connection>, StopToken) -> ();

pub struct ReplayWorkerThread
{
    handle: Option<JoinHandle<()>>,
    channel: Sender<Connection>,
    shutdown_token: Option<StopSource>,
}

impl ReplayWorkerThread {
    pub fn new(work: ThreadFn) -> Self {
        let (s, r) = channel(1);
        let shutdown_token = StopSource::new();
        let stop_token = shutdown_token.stop_token();
        let handle = thread::spawn(move || {
            work(r, stop_token)
        });
        ReplayWorkerThread {
            handle: Some(handle),
            channel: s,
            shutdown_token: Some(shutdown_token)
        }
    }
    pub fn get_channel(&self) -> Sender<Connection> { self.channel.clone() }
    pub fn shutdown(&mut self) { self.shutdown_token = None}
}

impl Drop for ReplayWorkerThread {
    fn drop(&mut self) {
        self.handle.take().unwrap().join().unwrap();    /* TODO log */
    }
}

/* TODO move elsewhere */
pub fn dummy_work(streams: Receiver<Connection>, shutdown_token: StopToken)
{
    let mut pool = LocalPool::new();
    pool.spawner().spawn(dummy_handle_connection(streams, shutdown_token)).unwrap();
    pool.run()
}

async fn do_nothing(c: Connection, shutdown_token: StopToken) {
    drop(c);
    let f1 = sleep(Duration::from_secs(1)).fuse();
    let f2 = shutdown_token.fuse();
    pin_mut!(f1, f2);
    select! {
        () = f1 => (),
        () = f2 => (),
    }
}

async fn dummy_handle_connection(streams: Receiver<Connection>, shutdown_token: StopToken)
{
    let bound_streams = shutdown_token.stop_stream(streams);
    bound_streams.for_each_concurrent(None, |c| { do_nothing(c, shutdown_token.clone())} ).await;
}
