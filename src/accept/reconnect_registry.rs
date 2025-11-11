use std::collections::HashMap;
use tokio::time::{Duration, Instant};

use rand::RngCore;

use super::reconnect_header::ReconnectToken;

impl ReconnectToken {
    pub fn random() -> Self {
        let mut data = [0u8; 16];
        rand::rng().fill_bytes(&mut data);
        Self {
            data
        }
    }
}

pub struct ReconnectEntry<T> {
    age: Instant,
    value: T,
}

impl<T> ReconnectEntry<T> {
    pub fn new(t: T) -> Self {
        Self {
            age: Instant::now(),
            value: t,
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

pub struct ReconnectRegistry<V> {
    items: HashMap<ReconnectToken, ReconnectEntry<V>>,
    cleanup_duration: Duration,
    last_cleanup: Instant,
}

impl<V> ReconnectRegistry<V> {
    pub fn new(cleanup_duration: Duration) -> Self {
        Self {
            items: HashMap::new(),
            cleanup_duration,
            last_cleanup: Instant::now(),
        }
    }

    fn cleanup(&mut self) {
        let now = Instant::now();
        if now - self.last_cleanup < self.cleanup_duration {
            return
        }
        let cd = self.cleanup_duration;
        self.items.retain(|_, v| now - v.age < 2 * cd);
        self.last_cleanup = now;
    }

    pub fn put(&mut self, entry: V) -> ReconnectToken {
        let mut token = ReconnectToken::random();
        while self.items.contains_key(&token) {
            token = ReconnectToken::random();
        }

        self.cleanup();
        self.items.insert(token.clone(), ReconnectEntry::new(entry));
        token
    }

    pub fn get_mut(&mut self, token: &ReconnectToken) -> Option<&mut V> {
        self.cleanup();
        self.items.get_mut(token).map(|i| i.get_mut())
    }
}

#[cfg(test)]
mod test {
    use crate::util::test::sleep_s;

    use super::*;

    #[tokio::test]
    async fn test_registry_simple_get_put() {
        let mut registry = ReconnectRegistry::<usize>::new(Duration::from_secs(5));
        let t1 = registry.put(1);
        let t2 = registry.put(2);

        assert!(t1 != t2);
        let mut t3 = ReconnectToken::random();
        while t3 == t1 || t3 == t2 {
            t3 = ReconnectToken::random();
        }

        assert!(*registry.get_mut(&t1).unwrap() == 1);
        assert!(*registry.get_mut(&t2).unwrap() == 2);
        assert!(registry.get_mut(&t3).is_none());
    }

    #[tokio::test]
    async fn test_registry_cleanup() {
        tokio::time::pause();

        let mut registry = ReconnectRegistry::<usize>::new(Duration::from_secs(5));
        let t1 = registry.put(1);

        // Before first cleanup
        sleep_s(3).await;
        assert!(*registry.get_mut(&t1).unwrap() == 1);
        // After first cleanup, should not die yet
        sleep_s(3).await;
        assert!(*registry.get_mut(&t1).unwrap() == 1);

        sleep_s(5).await;
        assert!(registry.get_mut(&t1).is_none());
    }
}
