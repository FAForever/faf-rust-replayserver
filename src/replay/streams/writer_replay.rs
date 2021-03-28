use std::{cell::RefCell, io::Write, rc::Rc};

use futures::Future;
use tokio::{io::AsyncReadExt, sync::Notify};
use tokio_util::sync::CancellationToken;

use crate::{
    error::ConnResult,
    server::connection::Connection,
    util::buf_deque::BufDeque,
    util::{buf_traits::DiscontiguousBuf, timeout::cancellable},
};

use super::ReplayHeader;

enum MaybeHeader {
    None,
    Some(ReplayHeader),
    Discarded,
}

pub struct WriterReplay {
    header: MaybeHeader,
    data: BufDeque,
    delayed_data_len: usize,
    header_notification: Rc<Notify>,
    finished_notification: Rc<Notify>,
    finished: bool,
}

impl WriterReplay {
    pub fn new() -> Self {
        Self {
            header: MaybeHeader::None,
            data: BufDeque::new(),
            delayed_data_len: 0,
            header_notification: Rc::new(Notify::new()),
            finished_notification: Rc::new(Notify::new()),
            finished: false,
        }
    }
    // First, functions for connection data writing.
    pub fn add_header(&mut self, h: ReplayHeader) {
        self.header = MaybeHeader::Some(h);
        self.header_notification.notify_waiters();
    }

    pub fn add_data(&mut self, buf: &[u8]) {
        self.data.write_all(buf).unwrap();
    }

    pub fn finish(&mut self) {
        self.finished = true;
        self.finished_notification.notify_waiters();
    }

    // Second, functions for delayed data updating.
    pub fn set_delayed_data_len(&mut self, new: usize) {
        debug_assert!(self.delayed_data_len <= new);
        self.delayed_data_len = new;
    }

    pub fn get_data(&self) -> &impl DiscontiguousBuf {
        &self.data
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    // Third, stuff used by the merge strategy.
    pub fn take_header(&mut self) -> ReplayHeader {
        if let MaybeHeader::Some(h) = std::mem::replace(&mut self.header, MaybeHeader::Discarded) {
            return h;
        } else {
            panic!("Cannot take header");
        }
    }

    pub fn get_delayed_data_len(&self) -> usize {
        self.delayed_data_len
    }

    pub fn wait_for_header(&self) -> impl Future<Output = ()> {
        debug_assert!(!matches!(self.header, MaybeHeader::Discarded));

        let has_header = matches!(self.header, MaybeHeader::Some(..));
        let wait = self.header_notification.clone();
        async move {
            if has_header {
                return;
            }
            wait.notified().await
        }
    }

    pub fn wait_until_finished(&self) -> impl Future<Output = ()> {
        let finished = self.finished;
        let wait = self.finished_notification.clone();
        async move {
            if finished {
                return;
            }
            wait.notified().await
        }
    }

    pub fn discard(&mut self, until: usize) {
        self.data.discard(until);
    }

    pub fn discard_all(&mut self) {
        self.data.discard(usize::MAX);
    }
}

pub async fn read_from_connection(
    me: Rc<RefCell<WriterReplay>>,
    c: &mut Connection,
    shutdown_token: CancellationToken,
) {
    cancellable(do_read_from_connection(&me, c), &shutdown_token).await;
    me.borrow_mut().finish();
}

async fn do_read_from_connection(
    me: &Rc<RefCell<WriterReplay>>,
    mut c: &mut Connection,
) -> ConnResult<()> {
    let header = ReplayHeader::from_connection(&mut c).await?;
    me.borrow_mut().add_header(header);
    let mut buf: Box<[u8]> = Box::new([0; 4096]);
    loop {
        let read = c.read(&mut *buf).await?;
        if read == 0 {
            break;
        }
        me.borrow_mut().add_data(&buf[0..read]);
    }
    Ok(())
}

pub type WReplayRef = Rc<RefCell<WriterReplay>>;
