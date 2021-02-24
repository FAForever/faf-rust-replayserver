use std::{cell::RefCell, io::Read, io::Write, rc::Rc};

use futures::Future;

use crate::{
    util::buf_list::BufList, util::buf_traits::DiscontiguousBuf, util::buf_traits::ReadAt,
    util::progress::ProgressKey, util::progress::ProgressTracker,
};

use super::{writer_replay::WriterReplay, ReplayHeader};

// To merged replay readers we give an interface that merges header and data.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MergedReplayPosition {
    Data(usize),
    Finished,
}

impl ProgressKey for MergedReplayPosition {
    fn bottom() -> Self {
        Self::Data(0)
    }

    fn top() -> Self {
        Self::Finished
    }
}

pub struct MergedReplay {
    data: BufList,
    header: Option<ReplayHeader>,
    delayed_progress: ProgressTracker<MergedReplayPosition>,
    delayed_data_len: usize,
}

impl MergedReplay {
    pub fn new() -> Self {
        Self {
            data: BufList::new(),
            header: None,
            delayed_progress: ProgressTracker::new(),
            delayed_data_len: 0,
        }
    }

    // The three values below are *delayed* data.
    fn header_len(&self) -> usize {
        self.get_header().map_or(0, |h| h.data.len())
    }

    pub fn data_len(&self) -> usize {
        self.delayed_data_len
    }

    pub fn len(&self) -> usize {
        self.delayed_data_len + self.header_len()
    }

    /* Counts both header and data bytes. */
    pub fn wait_for_byte(&self, until: usize) -> impl Future<Output = ()> {
        let f = self
            .delayed_progress
            .wait(MergedReplayPosition::Data(until));
        async {
            f.await;
        }
    }

    pub fn add_header(&mut self, header: ReplayHeader) {
        debug_assert!(self.delayed_progress.position() == MergedReplayPosition::Data(0));
        self.header = Some(header);
        self.delayed_progress
            .advance(MergedReplayPosition::Data(self.header_len()));
    }

    pub fn get_header(&self) -> Option<&ReplayHeader> {
        self.header.as_ref()
    }

    pub fn add_data(&mut self, writer: &WriterReplay, until: usize) {
        debug_assert!(self.delayed_progress.position() < MergedReplayPosition::Finished);
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
        debug_assert!(self.delayed_progress.position() < MergedReplayPosition::Finished);

        self.delayed_data_len = len;
        let pos = MergedReplayPosition::Data(self.header_len() + len);
        self.delayed_progress.advance(pos);
    }

    pub fn finish(&mut self) {
        self.delayed_progress
            .advance(MergedReplayPosition::Finished);
    }
}

impl ReadAt for MergedReplay {
    fn read_at(&self, mut start: usize, buf: &mut [u8]) -> std::io::Result<usize> {
        if start >= self.len() {
            return Ok(0);
        }
        debug_assert!(self.header.is_some());
        if start < self.header_len() {
            let mut data = &self.get_header().unwrap().data[start..];
            data.read(buf)
        } else {
            start -= self.header_len();
            let read_max = std::cmp::min(buf.len(), self.data_len() - start);
            self.data.read_at(start, &mut buf[..read_max])
        }
    }
}

pub type MReplayRef = Rc<RefCell<MergedReplay>>;

// TODO tests
