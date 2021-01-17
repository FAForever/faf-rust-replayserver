use std::io::Write;
use std::io::Read;

use futures::Future;

use crate::{async_utils::buf_list::BufList, replay::position::PositionTracker, replay::header::ReplayHeader, replay::position::StreamPosition, async_utils::buf_traits::DiscontiguousBuf, async_utils::buf_traits::ReadAt};

use super::writer_replay::WriterReplay;

// FIXME there's some overlap between this and WriterReplay. Then again, both are used somewhat
// differently, making them share code would probably be worse.

pub struct MergedReplay {
    data: BufList,
    header: Option<ReplayHeader>,
    delayed_progress: PositionTracker,
}

impl MergedReplay {
    pub fn new() -> Self {
        Self {
            data: BufList::new(),
            header: None,
            delayed_progress: PositionTracker::new(),
        }
    }

    pub fn add_header(&mut self, header: ReplayHeader) {
        debug_assert!(self.delayed_progress.position() == StreamPosition::START);
        self.header = Some(header);
        self.delayed_progress.advance(StreamPosition::DATA(0));
    }

    pub fn delayed_wait(&self, until: StreamPosition) -> impl Future<Output = StreamPosition> {
        self.delayed_progress.wait(until)
    }

    pub fn position(&self) -> StreamPosition {
        self.delayed_progress.position()
    }

    // FIXME this might fit better among buffer traits?
    pub fn add_data(&mut self, writer: &WriterReplay, until: usize) {
        debug_assert!(self.position() >= StreamPosition::DATA(0));
        debug_assert!(self.position() < StreamPosition::FINISHED(0));
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
        debug_assert!(self.position() >= StreamPosition::DATA(0));
        debug_assert!(self.position() < StreamPosition::FINISHED(0));
        let pos = StreamPosition::DATA(len);
        self.delayed_progress.advance(pos);
    }

    pub fn finish(&mut self) {
        let final_len = self.position().len();
        self.delayed_progress.advance(StreamPosition::FINISHED(final_len));
    }
}

impl ReadAt for MergedReplay {
    fn read_at(&self, start: usize, buf: &mut [u8]) -> std::io::Result<usize> {
        match &self.header {
            None => Ok(0),
            Some(h) => {
                if start < h.data.len() {
                    let mut chunk = &h.data[start..];
                    chunk.read(buf)
                } else {
                    self.data.read_at(start - h.data.len(), buf)
                }
            }
        }
    }
}
