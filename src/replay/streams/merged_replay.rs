use std::task::Context;
use std::task::Poll;
use std::{cell::RefCell, io::Write, rc::Rc};

use tokio::io::AsyncRead;

use crate::util::event::Event;
use crate::{
    util::buf_traits::ChunkedBuf,
    util::{buf_deque::BufDeque, buf_traits::ChunkedBufExt},
};

use super::ReplayStream;
use super::{writer_replay::WriterReplay, ReplayHeader};

pub struct MergedReplay {
    data: BufDeque,
    header: Option<ReplayHeader>,
    delayed_data_len: usize,
    finished: bool,
    read_event: Event,
}

impl ReplayStream for MergedReplay {
    type Buf = BufDeque;

    fn data_len(&self) -> usize {
        self.data.len()
    }

    fn delayed_data_len(&self) -> usize {
        self.delayed_data_len
    }

    fn get_data(&self) -> &Self::Buf {
        &self.data
    }

    fn is_finished(&self) -> bool {
        self.finished
    }
}

// Access to replay header + data.
impl ChunkedBuf for MergedReplay {
    fn len(&self) -> usize {
        self.delayed_data_len() + self.header_len()
    }

    fn get_chunk(&self, mut start: usize) -> &[u8] {
        if start < self.header_len() {
            &self.get_header().unwrap().data[start..]
        } else {
            start -= self.header_len();
            let delay_limit = self.delayed_data_len - start;

            let chunk = self.data.get_chunk(start);
            let read_max = std::cmp::min(chunk.len(), delay_limit);
            &chunk[..read_max]
        }
    }
}

impl MergedReplay {
    pub fn new() -> Self {
        Self {
            data: BufDeque::new(),
            header: None,
            delayed_data_len: 0,
            finished: false,
            read_event: Event::new(),
        }
    }

    pub fn get_header(&self) -> Option<&ReplayHeader> {
        self.header.as_ref()
    }

    pub fn add_header(&mut self, header: ReplayHeader) {
        debug_assert!(!self.finished);
        debug_assert!(self.data_len() == 0);
        self.header = Some(header);
        self.notify_read_event();
    }

    pub fn header_len(&self) -> usize {
        self.get_header().map_or(0, |h| h.data.len())
    }

    pub fn wait_for_read_event(&mut self, cx: &Context) {
        self.read_event.wait(cx);
    }

    fn notify_read_event(&mut self) {
        self.read_event.notify();
    }

    pub fn add_data(&mut self, writer: &WriterReplay, until: usize) {
        debug_assert!(!self.finished);
        debug_assert!(until <= writer.get_data().len());

        let writer_data = writer.get_data();
        let from = self.data.len();
        for chunk in writer_data.iter_chunks(from, until) {
            self.data.write_all(chunk).unwrap();
        }
    }

    pub fn advance_delayed_data(&mut self, len: usize) {
        debug_assert!(len <= self.data.len());
        debug_assert!(!self.finished);
        self.delayed_data_len = len;
        self.notify_read_event();
    }

    pub fn finish(&mut self) {
        self.finished = true;
        self.notify_read_event();
    }
}

pub type MReplayRef = Rc<RefCell<MergedReplay>>;

// AsyncRead for merged replay.
//
// Having all position/delayed/is_finished logic in poll_read guarantees that we won't cause some
// stupid race condition by awaiting while holding some replay state.
pub struct MReplayReader {
    replay: MReplayRef,
    position: usize,
}

impl MReplayReader {
    pub fn new(replay: MReplayRef) -> Self {
        Self { replay, position: 0 }
    }
}

impl AsyncRead for MReplayReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let mut r = self.replay.borrow_mut();

        if r.len() <= self.position {
            if r.is_finished() {
                Poll::Ready(Ok(()))
            } else {
                r.wait_for_read_event(cx);
                Poll::Pending
            }
        } else {
            // TODO maybe we could do a loop here
            let chunk = r.get_chunk(self.position);
            let copyable = std::cmp::min(chunk.len(), buf.remaining());
            buf.put_slice(&chunk[..copyable]);
            drop(r);
            self.position += copyable;
            Poll::Ready(Ok(()))
        }
    }
}

// NOTE: If we could implement AsyncBufReader, we could write replay data without extra copies. We
// can't though since we don't *own* the buffer, we have to borrow through a RefCell and can't
// return a bare &[u8]. I don't think there's a way around this.

// TODO tests
