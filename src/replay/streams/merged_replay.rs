use std::{cell::RefCell, io::Read, io::Write, rc::Rc};

use futures::Future;
use tokio::sync::Notify;

use crate::{
    util::buf_traits::DiscontiguousBuf,
    util::{buf_deque::BufDeque, buf_traits::ReadAt},
};

use super::{writer_replay::WriterReplay, ReplayHeader};

pub struct MergedReplay {
    data: BufDeque,
    header: Option<ReplayHeader>,
    delayed_data_len: usize,
    finished: bool,
    delayed_data_notification: Rc<Notify>,
}

impl MergedReplay {
    pub fn new() -> Self {
        Self {
            data: BufDeque::new(),
            header: None,
            delayed_data_len: 0,
            finished: false,
            delayed_data_notification: Rc::new(Notify::new()),
        }
    }

    pub fn header_len(&self) -> usize {
        self.get_header().map_or(0, |h| h.data.len())
    }

    pub fn delayed_data_len(&self) -> usize {
        self.delayed_data_len
    }

    pub fn delayed_len(&self) -> usize {
        self.delayed_data_len + self.header_len()
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn wait_for_more_data(&self) -> impl Future<Output = ()> {
        let wait = self.delayed_data_notification.clone();
        let finished = self.finished;
        async move {
            if finished {
                return;
            }
            wait.notified().await;
        }
    }

    pub fn add_header(&mut self, header: ReplayHeader) {
        debug_assert!(!self.finished);
        debug_assert!(self.get_data().len() == 0);
        self.header = Some(header);
        self.delayed_data_notification.notify_waiters();
    }

    pub fn get_header(&self) -> Option<&ReplayHeader> {
        self.header.as_ref()
    }

    pub fn add_data(&mut self, writer: &WriterReplay, until: usize) {
        debug_assert!(!self.finished);
        debug_assert!(until <= writer.get_data().len());

        let writer_data = writer.get_data();
        let mut cursor = self.data.len();
        while cursor < until {
            let mut chunk = writer_data.get_chunk(cursor);
            if chunk.len() > until - cursor {
                chunk = &chunk[..until - cursor];
            }
            self.data.write_all(chunk).unwrap();
            cursor += chunk.len();
        }
    }

    pub fn get_data(&self) -> &impl DiscontiguousBuf {
        &self.data
    }

    pub fn advance_delayed_data(&mut self, len: usize) {
        debug_assert!(len <= self.data.len());
        debug_assert!(!self.finished);
        self.delayed_data_len = len;
        self.delayed_data_notification.notify_waiters();
    }

    pub fn finish(&mut self) {
        self.finished = true;
        self.delayed_data_notification.notify_waiters();
    }
}

impl ReadAt for MergedReplay {
    fn read_at(&self, mut start: usize, buf: &mut [u8]) -> std::io::Result<usize> {
        if start >= self.delayed_len() {
            return Ok(0);
        }
        debug_assert!(self.header.is_some());
        if start < self.header_len() {
            let mut data = &self.get_header().unwrap().data[start..];
            data.read(buf)
        } else {
            start -= self.header_len();
            let read_max = std::cmp::min(buf.len(), self.delayed_data_len - start);
            self.data.read_at(start, &mut buf[..read_max])
        }
    }
}

pub type MReplayRef = Rc<RefCell<MergedReplay>>;

// TODO tests
