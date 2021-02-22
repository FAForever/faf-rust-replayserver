use std::{cell::RefCell, io::Write, rc::Rc};

use futures::Future;
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

use crate::{
    error::ConnResult,
    replay::position::PositionTracker,
    replay::position::StreamPosition,
    server::connection::Connection,
    util::buf_deque::BufDeque,
    util::buf_traits::BufWithDiscard,
    util::{buf_traits::DiscontiguousBuf, timeout::cancellable},
};

use super::ReplayHeader;

enum MaybeHeader {
    None,
    Some(ReplayHeader),
    Discarded(usize),
}

pub struct WriterReplay {
    header: MaybeHeader,
    data: BufDeque,
    progress: PositionTracker,
    delayed_data_progress: usize,
}

impl WriterReplay {
    pub fn new() -> Self {
        Self {
            header: MaybeHeader::None,
            data: BufDeque::new(),
            progress: PositionTracker::new(),
            delayed_data_progress: 0,
        }
    }
    pub fn add_header(&mut self, h: ReplayHeader) {
        debug_assert!(self.progress.position() == StreamPosition::START);
        self.header = MaybeHeader::Some(h);
        self.progress.advance(StreamPosition::DATA(0));
    }

    pub fn take_header(&mut self) -> ReplayHeader {
        let data_len = match &self.header {
            MaybeHeader::Some(h) => h.data.len(),
            _ => panic!("Cannot take header"),
        };
        if let MaybeHeader::Some(h) =
            std::mem::replace(&mut self.header, MaybeHeader::Discarded(data_len))
        {
            return h;
        } else {
            panic!("Cannot take header");
        }
    }

    pub fn add_data(&mut self, buf: &[u8]) {
        debug_assert!(self.position() >= StreamPosition::DATA(0));
        debug_assert!(self.position() < StreamPosition::FINISHED(0));
        self.data.write_all(buf).unwrap();
        self.progress.advance(self.position() + buf.len());
    }

    pub fn get_data(&self) -> &impl DiscontiguousBuf {
        &self.data
    }

    pub fn set_delayed_data_progress(&mut self, new: usize) {
        debug_assert!(self.delayed_data_progress <= new);
        self.delayed_data_progress = new;
    }

    pub fn get_delayed_data_progress(&self) -> usize {
        self.delayed_data_progress
    }

    pub fn discard(&mut self, until: usize) {
        self.data.discard(until);
    }

    pub fn discard_all(&mut self) {
        self.data.discard(usize::MAX);
    }

    pub fn finish(&mut self) {
        let final_len = self.position().len();
        self.progress.advance(StreamPosition::FINISHED(final_len));
    }

    pub fn wait(&self, until: StreamPosition) -> impl Future<Output = StreamPosition> {
        self.progress.wait(until)
    }

    pub fn position(&self) -> StreamPosition {
        self.progress.position()
    }

    pub fn is_finished(&self) -> bool {
        self.progress.position() >= StreamPosition::FINISHED(0)
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
    // TODO dep injection?
    let header = ReplayHeader::from_connection(&mut c).await?;
    {
        me.borrow_mut().add_header(header);
    }
    /* We can't modify inner.data in-place, merging might use it in the meantime
     * Maybe we could use a different structure? Can't be bothered rn
     * */
    let mut buf: Box<[u8]> = Box::new([0; 4096]);
    loop {
        let read = c.read(&mut *buf).await?;
        if read == 0 {
            break;
        }
        {
            me.borrow_mut().add_data(&buf[0..read]);
        }
    }
    Ok(())
}

pub type WReplayRef = Rc<RefCell<WriterReplay>>;
