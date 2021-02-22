use futures::Future;
use std::time::Duration;
use tokio::select;
use tokio_util::sync::CancellationToken;

pub async fn timeout<T, F: Future<Output = T>>(f: F, time: Duration) -> Option<T> {
    select! {
        res = f => Some(res),
        _ = tokio::time::sleep(time) => None,
    }
}

pub async fn cancellable<T, F: Future<Output = T>>(f: F, token: &CancellationToken) -> Option<T> {
    select! {
        res = f => Some(res),
        _ = token.cancelled() => None,
    }
}

#[cfg(test)]
mod test {
    use tokio::join;

    use super::*;

    #[tokio::test]
    async fn timeout_normal() {
        tokio::time::pause();
        let work = async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            1
        };
        let res = timeout(work, Duration::from_secs(10)).await;
        assert!(res == Some(1));
    }

    #[tokio::test]
    async fn timeout_timeout() {
        tokio::time::pause();
        let work = async {
            tokio::time::sleep(Duration::from_secs(20)).await;
            1
        };
        let res = timeout(work, Duration::from_secs(10)).await;
        assert!(res == None);
    }

    #[tokio::test]
    async fn cancel_normal() {
        tokio::time::pause();
        let token = CancellationToken::new();

        let work = async {
            tokio::time::sleep(Duration::from_secs(5)).await;
        };
        let cancel = async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            token.cancel();
        };
        let cancellable = cancellable(work, &token);
        let (res, _) = join!(cancellable, cancel);
        assert!(res.is_some());
    }
    
    #[tokio::test]
    async fn cancel_cancelled() {
        tokio::time::pause();
        let token = CancellationToken::new();

        let work = async {
            tokio::time::sleep(Duration::from_secs(20)).await;
        };
        let cancel = async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            token.cancel();
        };
        let cancellable = cancellable(work, &token);
        let (res, _) = join!(cancellable, cancel);
        assert!(res.is_none());
    }
}
