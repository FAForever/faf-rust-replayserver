use futures::{Future, FutureExt};
use tokio::sync::watch::{channel, Receiver, Sender};

// This struct allows us to wait for and notify an event. Waiting on wait() will wait until
// notify() is called at any time after wait() was called. Tokio doesn't give us a primitive with
// these exact semantics, so we piece together our own.
//
// Example usage of an object A with an event can be as follows:
// * Task one gets a reference to A, does some work with it. It decides it has to wait until A
//   changes state. It calls wait, then awaits on output, releasing the reference.
// * Task two gets a reference to A, changes its state, calls notify(). This can all happen
//   *before* task one awaits, but notify and wait can't be called in parallel.
// * Task one is waken up.
pub struct Event(Sender<()>, Receiver<()>);

impl Event {
    pub fn new() -> Self {
        let (s, r) = channel(());
        Self(s, r)
    }

    pub fn wait(&self) -> impl Future<Output = ()> {
        let mut rc = self.1.clone();
        // Cloning receiver retains the last value we watched.
        // Observe the last value now so we wait until next one.
        rc.changed().now_or_never();
        async move {
            rc.changed().await.unwrap();
        }
    }

    pub fn notify(&mut self) {
        self.0.send(()).unwrap();
    }
}
