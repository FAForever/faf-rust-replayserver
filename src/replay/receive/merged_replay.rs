use std::io::Write;

use futures::Future;

use crate::{async_utils::buf_list::BufList, replay::position::PositionTracker, replay::header::ReplayHeader, replay::position::StreamPosition};

struct MergedReplay {
    data: BufList,
    header_size: Option<usize>,
    delayed_progress: PositionTracker,
}

impl MergedReplay {
    pub fn new() -> Self {
        Self {
            data: BufList::new(),
            header_size: None,
            delayed_progress: PositionTracker::new(),
        }
    }

    pub fn add_header(&mut self, header: &ReplayHeader) {
        debug_assert!(self.delayed_progress.position() == StreamPosition::START);
        self.data.write_all(header.data.as_ref()).unwrap();
        self.header_size = Some(header.data.len());
    }

    pub fn wait(&self, until: StreamPosition) -> impl Future<Output = StreamPosition> {
        self.delayed_progress.wait(until)
    }

    pub fn position(&self) -> StreamPosition {
        self.delayed_progress.position()
    }

    pub fn add_data(&mut self, buf: &[u8]) {
        debug_assert!(self.position() >= StreamPosition::DATA(0));
        debug_assert!(self.position() < StreamPosition::FINISHED(0));
        self.data.write_all(buf).unwrap();
        self.delayed_progress.advance(self.position() + buf.len());
    }
}
