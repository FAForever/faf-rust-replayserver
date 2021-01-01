use crate::async_utils::progress::{ProgressKey, ProgressTracker};

/* Tracking replay stream position.
 */

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StreamPosition {
    START,
    HEADER,
    DATA(usize),
    FINISHED,
}

impl ProgressKey for StreamPosition {
    fn bottom() -> Self {StreamPosition::START}
    fn top() -> Self {StreamPosition::FINISHED}
}

pub type PositionTracker = ProgressTracker<StreamPosition>;

#[cfg(test)]
mod test {

use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::join;
use super::{PositionTracker, StreamPosition};

    async fn advance_some(t: &PositionTracker, mut c: Receiver<()>) {
        t.advance(StreamPosition::HEADER);
        c.recv().await;
        t.advance(StreamPosition::DATA(5));
        c.recv().await;
        t.advance(StreamPosition::DATA(10));
        c.recv().await;
        t.advance(StreamPosition::FINISHED);
        c.recv().await;
    }

    async fn await_test(t: &PositionTracker, when: StreamPosition, c: Sender<()>) {
        t.wait(when).await;
        assert!(t.position() == when);
        c.send(()).await.unwrap();
    }

    #[tokio::test]
    async fn simple_test() {
        let tracker = PositionTracker::new();
        let (send, recv) = channel::<()>(1);
        join! {
            await_test(&tracker, StreamPosition::HEADER, send.clone()),
            await_test(&tracker, StreamPosition::DATA(5), send.clone()),
            await_test(&tracker, StreamPosition::DATA(10), send.clone()),
            await_test(&tracker, StreamPosition::FINISHED, send.clone()),
            advance_some(&tracker, recv),
        };
    }
}
