use std::task::Context;
use std::task::Waker;

// Simple structure for registering and releasing wakers.
//
// Example usage of an object A with an event is as follows:
// * A owns an Event.
// * Task one gets a reference to A and does some work with it. It decides it has to wait until A
//   changes state. Through A, it registers itself in the event.
// * Task two gets a reference to A, changes its state and calls notify() through A, notifying all
//   wakers.
// * Task one is woken up.
pub struct Event(Vec<Waker>);
impl Event {
    pub fn new() -> Self {
        Self(vec![])
    }

    pub fn wait(&mut self, cx: &Context) {
        self.0.push(cx.waker().clone());
    }

    pub fn notify(&mut self) {
        for w in self.0.drain(0..) {
            w.wake();
        }
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        self.notify()
    }
}
