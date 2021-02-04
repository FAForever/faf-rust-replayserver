use std::ops::Add;

use crate::util::progress::{ProgressKey, ProgressTracker};

/* Tracking replay stream position.
 */

#[derive(Clone, Copy)]
pub enum StreamPosition {
    START,
    // HEADER is distinct from DATA(0). The earlier happens once ("header has arrived"), the latter
    // might happen multiple times ("still at zero data after the header").
    HEADER,
    DATA(usize),
    FINISHED(usize), // Final size, not used for cmp / sort
}

impl StreamPosition {
    pub fn len(&self) -> usize {
        match *self {
            Self::DATA(s) => s,
            Self::FINISHED(s) => s,
            Self::START | Self::HEADER => 0,
        }
    }
    fn variant_order(&self) -> u8 {
        match *self {
            Self::START => 0,
            Self::HEADER => 1,
            Self::DATA(..) => 2,
            Self::FINISHED(..) => 3,
        }
    }
}

impl PartialEq for StreamPosition {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Self::START, Self::START) => true,
            (Self::HEADER, Self::HEADER) => true,
            (Self::FINISHED(..), Self::FINISHED(..)) => true,
            (Self::DATA(u), Self::DATA(v)) => u == v,
            _ => false,
        }
    }
}
impl Eq for StreamPosition {}
impl PartialOrd for StreamPosition {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for StreamPosition {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (*self, *other) {
            (Self::DATA(u), Self::DATA(v)) => u.cmp(&v),
            (one, other) => one.variant_order().cmp(&other.variant_order()),
        }
    }
}

impl Add<usize> for StreamPosition {
    type Output = Self;
    fn add(self, v: usize) -> Self {
        match self {
            Self::DATA(p) => Self::DATA(p + v),
            _ => self,
        }
    }
}

impl ProgressKey for StreamPosition {
    fn bottom() -> Self {
        StreamPosition::START
    }
    fn top() -> Self {
        StreamPosition::FINISHED(0)
    }
}

pub type PositionTracker = ProgressTracker<StreamPosition>;

#[cfg(test)]
mod test {
    use super::{PositionTracker, StreamPosition};
    use std::cell::RefCell;
    use tokio::join;
    use tokio::sync::mpsc::{channel, Receiver, Sender};

    async fn advance_some(t: &RefCell<PositionTracker>, mut c: Receiver<()>) {
        t.borrow_mut().advance(StreamPosition::DATA(0));
        c.recv().await;
        t.borrow_mut().advance(StreamPosition::DATA(5));
        c.recv().await;
        t.borrow_mut().advance(StreamPosition::DATA(10));
        c.recv().await;
        t.borrow_mut().advance(StreamPosition::FINISHED(10));
        c.recv().await;
    }

    async fn await_test(t: &RefCell<PositionTracker>, when: StreamPosition, c: Sender<()>) {
        /* Borrow lives until the end of expression, therefore await is a separate one. */
        let f = t.borrow().wait(when);
        f.await;
        let p = t.borrow().position();
        assert!(p == when);
        c.send(()).await.unwrap();
    }

    #[tokio::test]
    async fn simple_test() {
        let tracker = RefCell::new(PositionTracker::new());
        let (send, recv) = channel::<()>(1);
        join! {
            await_test(&tracker, StreamPosition::DATA(0), send.clone()),
            await_test(&tracker, StreamPosition::DATA(5), send.clone()),
            await_test(&tracker, StreamPosition::DATA(10), send.clone()),
            await_test(&tracker, StreamPosition::FINISHED(10), send.clone()),
            advance_some(&tracker, recv),
        };
    }
}
