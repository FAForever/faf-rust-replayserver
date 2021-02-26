use futures::Future;
use tokio::{select, sync::watch, time::Duration};

pub struct EmptyCounter {
    counter: watch::Sender<usize>,
    watcher: watch::Receiver<usize>,
}

impl EmptyCounter {
    pub fn new() -> Self {
        let (counter, watcher) = watch::channel(0);
        Self {
            counter,
            watcher
        }
    }

    pub fn inc(&self) {
        let old = *self.counter.borrow();
        self.counter.send(old + 1).ok();
    }

    pub fn dec(&self) {
        let old = *self.counter.borrow();
        self.counter.send(old - 1).ok();
    }

    async fn await_new_count(watcher: &mut watch::Receiver<usize>) -> Option<usize> {
        watcher.changed().await.ok().and(Some(*watcher.borrow()))
    }

    pub fn wait_until_empty_for(&self, timeout: Duration) -> impl Future<Output = ()> {
        let mut watcher = self.watcher.clone();
        async move {
            let mut last_count = Some(*watcher.borrow());
            while !last_count.is_none() {
                if last_count != Some(0) {
                    last_count = Self::await_new_count(&mut watcher).await;
                } else {
                    last_count = select! {
                        r = Self::await_new_count(&mut watcher) => r,
                        _ = tokio::time::sleep(timeout) => Some(*watcher.borrow()).filter(|c| *c != 0),
                    }
                }
            }
        }
    }

    pub fn wait_until_empty(&self) -> impl Future<Output = ()> {
        self.wait_until_empty_for(Duration::from_secs(0))
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

    #[tokio::test]
    async fn test_empty_counter_wait_twice() {
        timeout(1000, async {
            let counter = EmptyCounter::new();
            let wait_empty = async {
                let before = Instant::now();
                counter
                    .wait_until_empty_for(Duration::from_millis(100))
                    .await;
                counter.wait_until_empty().await;
                let after = Instant::now();
                assert!(after - before > Duration::from_millis(150));
                assert!(after - before < Duration::from_millis(300));
            };
            let counter_1 = async {
                hold_counter(&counter, 0, 100).await;
            };
            join! {
                counter_1,
                wait_empty,
            };
        }).await;
    }
}
