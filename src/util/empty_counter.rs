use futures::Future;
use std::rc::Weak;
use std::{cell::RefCell, rc::Rc, task::Waker};
use tokio::time::{Duration, Instant};

mod private {
    use super::*;
    pub struct Inner {
        pub count: usize,
        pub wakers: Vec<Weak<RefCell<Option<Waker>>>>,
    }

    impl Inner {
        pub fn new() -> Self {
            Self {
                count: 0,
                wakers: Vec::new(),
            }
        }

        pub fn waker_token(&mut self) -> Rc<RefCell<Option<Waker>>> {
            let maybe_waker = Rc::new(RefCell::new(None));
            self.wakers.push(Rc::downgrade(&maybe_waker));
            maybe_waker
        }

        pub fn wake(&mut self) {
            self.wakers.retain(|w| match w.upgrade() {
                None => false,
                Some(p) => {
                    if let Some(waker) = p.borrow_mut().take() {
                        waker.wake();
                    }
                    true
                }
            });
        }

        pub fn is_empty(&self) -> bool {
            self.count == 0
        }

        pub fn inc(&mut self) {
            self.count += 1;
            if self.count == 1 {
                self.wake();
            }
        }

        pub fn dec(&mut self) {
            self.count -= 1;
            if self.count == 0 {
                self.wake();
            }
        }
    }
}

use private::Inner;

// Connection counter for writer connections.
//
// Writer coroutines call `guard` to count the total number of connections.
// Connection merger uses `wait_until_empty_for` to wait until there have been no connections
// for `timeout` time.
impl EmptyCounter {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(Inner::new())),
        }
    }

    pub fn inc(&self) {
        self.inner.borrow_mut().inc();
    }

    pub fn dec(&self) {
        self.inner.borrow_mut().dec();
    }

    pub fn wait_until_empty_for(&self, timeout: Duration) -> WaitForEmptyFuture {
        return WaitForEmptyFuture::new(self.inner.clone(), timeout);
    }

    pub fn wait_until_empty(&self) -> WaitForEmptyFuture {
        return WaitForEmptyFuture::new(self.inner.clone(), Duration::from_secs(0));
    }
}

pub struct EmptyCounter {
    inner: Rc<RefCell<Inner>>,
}

pub struct WaitForEmptyFuture {
    inner: Rc<RefCell<Inner>>,
    waker: Rc<RefCell<Option<Waker>>>,
    timeout: Duration,
    // We can't drop the timer when we don't need it because of structural pinning. Guard it with a
    // bool so we don't return when it times out but counter is not empty.
    timer: tokio::time::Sleep,
    counter_was_empty: bool,
}

impl WaitForEmptyFuture {
    pub fn new(inner: Rc<RefCell<Inner>>, timeout: Duration) -> Self {
        let waker = inner.borrow_mut().waker_token();
        Self {
            inner,
            waker,
            timeout,
            timer: tokio::time::sleep(timeout),
            counter_was_empty: false,
        }
    }
}

impl Future for WaitForEmptyFuture {
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let me = unsafe { self.get_unchecked_mut() };

        let mut waker = me.waker.borrow_mut();
        let pinned_timer = unsafe { std::pin::Pin::new_unchecked(&mut me.timer) };
        if waker.is_none() {
            // Woken up by counter state changing.
            *waker = Some(cx.waker().clone());
            pinned_timer.reset(Instant::now() + me.timeout);
            me.counter_was_empty = me.inner.borrow().is_empty();
            std::task::Poll::Pending
        } else {
            // Woken up by timer.
            if let std::task::Poll::Ready(_) = pinned_timer.poll(cx) {
                if me.counter_was_empty {
                    // We weren't woken up by counter for the whole duration of the timer, so it
                    // was empty for around that long. We're done.
                    return std::task::Poll::Ready(());
                }
            }
            std::task::Poll::Pending
        }
    }
}

#[cfg(test)]
mod test {
    use super::EmptyCounter;
    use futures::Future;
    use std::{cell::Cell, cell::RefCell, rc::Rc};
    use tokio::{join, time::sleep, time::Duration, time::Instant};

    async fn hold_counter(c: &EmptyCounter, wait_ms: u64, hold_end_ms: u64) {
        sleep(Duration::from_millis(wait_ms)).await;
        c.inc();
        sleep(Duration::from_millis(hold_end_ms - wait_ms)).await;
        c.dec();
    }

    // CAVEAT: tokio automagically advances the clock when time is paused. No need to manually
    // adjust it.
    async fn timeout(ms: u64, test: impl Future<Output = ()>) {
        tokio::time::pause();
        if let Err(_) = tokio::time::timeout(Duration::from_millis(ms), test).await {
            panic!("Timeout");
        }
    }

    #[tokio::test]
    async fn test_empty_counter_one_waiter() {
        timeout(300, async {
            let counter = EmptyCounter::new();
            let awakened = Cell::new(false);
            let wait_empty = async {
                let before = Instant::now();
                counter
                    .wait_until_empty_for(Duration::from_millis(100))
                    .await;
                awakened.set(true);
                let after = Instant::now();
                assert!(after - before > Duration::from_millis(120));
                assert!(after - before < Duration::from_millis(200));
            };
            let keep_counter = async {
                hold_counter(&counter, 20, 70).await;
                assert!(!awakened.get());
            };
            join! {
                keep_counter,
                wait_empty,
            };
        })
        .await
    }

    #[tokio::test]
    async fn test_empty_counter_no_waiters() {
        timeout(300, async {
            let counter = EmptyCounter::new();
            let awakened = Rc::new(RefCell::new(false));
            let wait_empty = async {
                let before = Instant::now();
                counter
                    .wait_until_empty_for(Duration::from_millis(100))
                    .await;
                *awakened.borrow_mut() = true;
                let after = Instant::now();
                assert!(after - before > Duration::from_millis(90));
                assert!(after - before < Duration::from_millis(200));
            };
            join! {
                wait_empty,
            };
        })
        .await
    }

    #[tokio::test]
    async fn test_empty_counter_multiple_waiters_breaks() {
        // |------|           |-------------|         |------|         |    |DONE      |---------|
        // 0     50          120           220       300    350       420  450        500       600
        timeout(1000, async {
            let counter = EmptyCounter::new();
            let awakened = Cell::new(false);
            let wait_empty = async {
                let before = Instant::now();
                counter
                    .wait_until_empty_for(Duration::from_millis(100))
                    .await;
                awakened.set(true);
                let after = Instant::now();
                assert!(after - before > Duration::from_millis(420));
                assert!(after - before < Duration::from_millis(500));
            };
            let counter_1 = async {
                hold_counter(&counter, 0, 50).await;
                assert!(!awakened.get());
            };
            let counter_2 = async {
                hold_counter(&counter, 120, 220).await;
                assert!(!awakened.get());
            };
            let counter_3 = async {
                hold_counter(&counter, 300, 350).await;
                assert!(!awakened.get());
            };
            let counter_4 = async {
                hold_counter(&counter, 500, 600).await;
                assert!(awakened.get());
            };
            join! {
                counter_1,
                counter_2,
                counter_3,
                counter_4,
                wait_empty,
            };
        })
        .await
    }

    #[tokio::test]
    async fn test_empty_counter_multiple_waiters_concurrent() {
        // |------|===========|-------------|         |   |DONE |=========|
        // 0     50          120           220       300 320   350       420
        timeout(1000, async {
            let counter = EmptyCounter::new();
            let awakened = Cell::new(false);
            let wait_empty = async {
                let before = Instant::now();
                counter
                    .wait_until_empty_for(Duration::from_millis(100))
                    .await;
                awakened.set(true);
                let after = Instant::now();
                assert!(after - before > Duration::from_millis(300));
                assert!(after - before < Duration::from_millis(350));
            };
            let counter_1 = async {
                hold_counter(&counter, 0, 120).await;
                assert!(!awakened.get());
            };
            let counter_2 = async {
                hold_counter(&counter, 50, 220).await;
                assert!(!awakened.get());
            };
            let counter_3 = async {
                hold_counter(&counter, 350, 420).await;
                assert!(awakened.get());
            };
            let counter_4 = async {
                hold_counter(&counter, 350, 420).await;
                assert!(awakened.get());
            };
            join! {
                counter_1,
                counter_2,
                counter_3,
                counter_4,
                wait_empty,
            };
        })
        .await
    }
}
