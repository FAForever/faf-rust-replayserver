use async_channel::{unbounded,Sender,Receiver};
use std::thread;
use std::thread::JoinHandle;

pub struct WorkerThread<T> {
    pub handle: JoinHandle<()>,
    pub channel: Sender<T>,
}

pub fn create_worker_thread<T: 'static + Send>(work: fn(Receiver<T>) -> ()) -> WorkerThread<T> {
    let (s, r) : (Sender<T>, Receiver<T>) = unbounded();
    let handle = thread::spawn(move || {work(r)});
    WorkerThread { handle: handle, channel: s}
}
