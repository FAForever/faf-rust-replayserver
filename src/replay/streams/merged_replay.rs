use std::{cell::RefCell, io::Write, rc::Rc};

use futures::Future;

use crate::{
    replay::position::PositionTracker, replay::position::StreamPosition, util::buf_list::BufList,
    util::buf_traits::DiscontiguousBuf,
};

use super::{writer_replay::WriterReplay, ReplayHeader};

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

    pub fn get_header(&self) -> Option<&ReplayHeader> {
        self.header.as_ref()
    }

    pub fn wait(&self, until: StreamPosition) -> impl Future<Output = StreamPosition> {
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
        self.delayed_progress
            .advance(StreamPosition::FINISHED(final_len));
    }
}

pub type MReplayRef = Rc<RefCell<MergedReplay>>;
