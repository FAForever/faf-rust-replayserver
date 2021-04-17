use std::{cell::RefCell, collections::VecDeque};

use crate::replay::streams::WReplayRef;
use crate::util::buf_traits::DiscontiguousBuf;

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

impl StreamDelay {
    pub fn new(delay_s: Duration, sleep_s: Duration) -> Self {
        Self { delay_s, sleep_s }
    }

    pub async fn track(&self, replay: &WReplayRef, strategy: &RefCell<impl MergeStrategy>, token: u64) {
        let mut pos_queue = PositionHistory::new(self.delay_s, self.sleep_s);
        let mut prev_current = 0;
        let mut prev_delayed = 0;
        loop {
            let current = replay.borrow().get_data().len();
            let delayed = pos_queue.push_and_get_delayed(current);
            replay.borrow_mut().set_delayed_data_len(delayed);
            if (current, delayed) != (prev_current, prev_delayed) {
                strategy.borrow_mut().replay_data_updated(token);
            }
            prev_current = current;
            prev_delayed = delayed;
            pos_queue.wait_cycle().await;
        }
    }

    pub fn set_to_end(&self, replay: &WReplayRef, strategy: &RefCell<impl MergeStrategy>, token: u64) {
        let final_len = replay.borrow_mut().get_data().len();
        let last_delayed_len = replay.borrow_mut().get_delayed_data_len();
        if final_len > last_delayed_len {
            replay.borrow_mut().set_delayed_data_len(final_len);
            strategy.borrow_mut().replay_data_updated(token);
        }
    }
}
