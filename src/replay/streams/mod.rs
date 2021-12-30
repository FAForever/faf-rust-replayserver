mod header;
mod merged_replay;
mod writer_replay;

use std::cell::Ref;
use std::cell::RefCell;
use std::rc::Rc;

use crate::util::buf_traits::ChunkedBuf;

pub use self::header::ReplayHeader;
pub use self::merged_replay::{MReplayReader, MReplayRef, MergedReplay};
pub use self::writer_replay::{read_data, read_header, WReplayRef, WriterReplay};

// Some common behaviour for WriterReplay and MergedReplay, that is:
//
// * A ChunkedBuf for replay data that you can read.
// * Total length of replay data.
// * "Delayed" length of replay data that's less than total length. In general this means that data
//   beyond that length should not be available to readers yet.
// * Whether the replay is "finished" or not. Finished replays will never generate more data.
pub trait ReplayStream {
    type Buf: ChunkedBuf;
    fn data_len(&self) -> usize;
    fn delayed_data_len(&self) -> usize;
    fn get_data(&self) -> &Self::Buf;
    fn is_finished(&self) -> bool;
}

// Convenience trait for accessing all of the above through a RefCell.
pub trait ReplayStreamRef {
    type Buf: ChunkedBuf;
    fn data_len(&self) -> usize;
    fn delayed_data_len(&self) -> usize;
    fn is_finished(&self) -> bool;
    fn get_data(&self) -> Ref<'_, Self::Buf>;
}

impl<S: ReplayStream> ReplayStreamRef for Rc<RefCell<S>> {
    type Buf = S::Buf;
    fn data_len(&self) -> usize {
        self.borrow().data_len()
    }

    fn delayed_data_len(&self) -> usize {
        self.borrow().delayed_data_len()
    }

    fn get_data(&self) -> Ref<'_, Self::Buf> {
        Ref::map(self.borrow(), |x| x.get_data())
    }

    fn is_finished(&self) -> bool {
        self.borrow().is_finished()
    }
}
