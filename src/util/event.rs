use std::{cell::UnsafeCell, rc::Rc};

use futures::Future;
use tokio::sync::Notify;

// This struct allows us to wait for and notify an event. Waiting on wait() will wait until
// notify() is called at any time after wait() was called. Tokio doesn't give us a primitive with
// these exact semantics, so we piece together our own.
//
// Implementation-wise, it's basically simplified Watch, except it's !Sync and !Send.
//
// Example usage of an object A with an event can be as follows:
// * Task one gets a reference to A, does some work with it. It decides it has to wait until A
//   changes state. It calls wait, then awaits on output, releasing the reference.
// * Task two gets a reference to A, changes its state, calls notify(). This can all happen
//   *before* task one awaits, but notify and wait can't be called in parallel.
// * Task one is waken up.
pub struct Event {
    notify: Rc<Notify>,
    version: Rc<UnsafeCell<usize>>,
}

pub struct Waiter {
    notify: Rc<Notify>,
    version: Rc<UnsafeCell<usize>>,
    last: usize,
}

impl Waiter {
    pub async fn wait(self) {
        loop {
            let notify = self.notify.notified();
            // We're in a single thread and not reentrant, it's fine
            let version = unsafe { *self.version.get() };
            if self.last < version {
                return
            } else {
                notify.await
            }
        }
    }

}

impl Event {
    pub fn new() -> Self {
        Self {
            notify: Rc::new(Notify::new()),
            version: Rc::new(UnsafeCell::new(0))
        }
    }

    pub fn wait(&self) -> impl Future<Output = ()> {
        let w = Waiter {
            notify: self.notify.clone(),
            version: self.version.clone(),
            last: unsafe { *self.version.get() }
        };
        w.wait()
    }

    pub fn notify(&mut self) {
        unsafe { *self.version.get() += 1 };
        self.notify.notify_waiters();
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        self.notify()
    }
}

#[cfg(test)]
mod test {
    use std::sync::{Arc, Mutex};
    use futures::FutureExt;
    use tokio::sync::Barrier;

    use super::*;

    #[tokio::test]
    async fn test_wait_then_notify_then_await() {
        let e = Arc::new(Mutex::new(Event::new()));
        let e2 = e.clone();
        let b = Arc::new(Barrier::new(2));
        let b2 = b.clone();

        let set_once = async move {
            b.wait().await;

            e.lock().unwrap().notify();
            b.wait().await;
        };
        let get_once = async move {
            let w = e2.lock().unwrap().wait();
            b2.wait().await;

            b2.wait().await;

            assert!(w.now_or_never().is_some());
        };

        tokio::join! {
            set_once,
            get_once,
        };
    }

    #[tokio::test]
    async fn test_notify_then_wait() {
        let e = Arc::new(Mutex::new(Event::new()));
        let e2 = e.clone();
        let b = Arc::new(Barrier::new(2));
        let b2 = b.clone();

        let set_once = async move {
            e.lock().unwrap().notify();
            b.wait().await;
        };
        let get_once = async move {
            b2.wait().await;
            let w = e2.lock().unwrap().wait();
            assert!(w.now_or_never().is_none());
        };

        tokio::join! {
            set_once,
            get_once,
        };
    }
}
