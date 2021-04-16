use std::{cell::RefCell, collections::VecDeque};

use crate::util::buf_traits::DiscontiguousBuf;
use crate::{replay::streams::WReplayRef, util::timeout::until};

use tokio::time::Duration;

use super::merge_strategy::MergeStrategy;

pub struct PositionHistory {
    sleep_s: Duration,
    history_size: usize,
    queue: VecDeque<usize>,
}

impl PositionHistory {
    pub fn new(delay_s: Duration, sleep_s: Duration) -> Self {
        let history_size = Self::history_size(delay_s, sleep_s);
        Self {
            sleep_s,
            history_size,
            queue: VecDeque::new(),
        }
    }

    fn history_size(delay_s: Duration, sleep_s: Duration) -> usize {
        /* Right after placing a new value, Nth element in the deque (counting from 0) spent N
         * sleep cycles in it. We want:
         *     N * sleep_s >= delay_s
         *     N >= ceil(delay_s / sleep_s)
         *     Deque size >= ceil(delay_s / sleep_s) + 1
         */
        (delay_s.as_secs_f64() / sleep_s.as_secs_f64()).ceil() as usize + 1
    }

    /* Push current data position and receive a position from delay_s seconds back. */
    pub fn push_and_get_delayed(&mut self, current_len: usize) -> usize {
        self.queue.push_back(current_len);
        if self.queue.len() > self.history_size {
            self.queue.pop_front();
        }
        *self.queue.front().unwrap()
    }

    pub async fn wait_cycle(&self) {
        tokio::time::sleep(self.sleep_s).await;
    }
}

pub struct StreamDelay {
    delay_s: Duration,
    sleep_s: Duration,
}

// This does two things:
// * Sets writer stream's delayed data position. The position is updated roughly each sleep_s
//   seconds and is set to data position delay_s seconds ago. Once the replay ends, the delayed
//   position is set to real data position (since we don't need to anti-spoiler a replay that
//   already ended).
// * Calls merge strategy functions at the right times.
impl StreamDelay {
    pub fn new(delay_s: Duration, sleep_s: Duration) -> Self {
        Self { delay_s, sleep_s }
    }

    pub async fn update_delayed_data_and_drive_merge_strategy(
        &self,
        replay: &WReplayRef,
        strategy: &RefCell<impl MergeStrategy>,
    ) {
        let driver = StreamDelayContext::new(self, replay, strategy);
        driver.update_delayed_data_and_drive_merge_strategy().await;
    }
}

struct StreamDelayContext<'a, T: MergeStrategy> {
    delay_s: Duration,
    sleep_s: Duration,
    replay: &'a WReplayRef,
    strategy: &'a RefCell<T>,
    token: u64,
}

impl<'a, T: MergeStrategy> StreamDelayContext<'a, T> {
    pub fn new(delay: &StreamDelay, replay: &'a WReplayRef, strategy: &'a RefCell<T>) -> Self {
        let token = strategy.borrow_mut().replay_added(replay.clone());
        Self {
            delay_s: delay.delay_s,
            sleep_s: delay.sleep_s,
            replay,
            strategy,
            token,
        }
    }

    async fn notify_on_header(&self) {
        let f = self.replay.borrow().wait_for_header();
        f.await;
        self.strategy.borrow_mut().replay_header_added(self.token);
    }

    async fn update_delayed_data(&self) -> ! {
        let mut pos_queue = PositionHistory::new(self.delay_s, self.sleep_s);
        let mut prev_current = 0;
        let mut prev_delayed = 0;
        loop {
            let current = self.replay.borrow().get_data().len();
            let delayed = pos_queue.push_and_get_delayed(current);
            self.replay.borrow_mut().set_delayed_data_len(delayed);
            if (current, delayed) != (prev_current, prev_delayed) {
                self.strategy.borrow_mut().replay_data_updated(self.token);
            }
            prev_current = current;
            prev_delayed = delayed;
            pos_queue.wait_cycle().await;
        }
    }

    async fn after_finished(&self) {
        let final_len = self.replay.borrow_mut().get_data().len();
        self.replay.borrow_mut().set_delayed_data_len(final_len);
        let mut s = self.strategy.borrow_mut();
        s.replay_data_updated(self.token);
        s.replay_removed(self.token);
    }

    async fn update_delayed_data_and_drive_merge_strategy(&self) {
        let replay_end = self.replay.borrow().wait_until_finished();
        until(
            async {
                self.notify_on_header().await;
                self.update_delayed_data().await;
            },
            replay_end,
        )
        .await;
        self.after_finished().await;
    }
}
