use futures::Future;
use tokio::select;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

pub async fn until<T1, F1, T2, F2>(f1: F1, f2: F2) -> Option<T1>
where
    F1: Future<Output = T1>,
    F2: Future<Output = T2>,
{
    select! {
        res = f1 => Some(res),
        _ = f2 => None,
    }
}

pub async fn timeout<T, F: Future<Output = T>>(f: F, time: Duration) -> Option<T> {
    until(f, tokio::time::sleep(time)).await
}

pub async fn cancellable<T, F: Future<Output = T>>(f: F, token: &CancellationToken) -> Option<T> {
    until(f, token.cancelled()).await
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::util::test::sleep_s;
    use tokio::join;

    #[tokio::test]
    async fn timeout_normal() {
        tokio::time::pause();
        let work = async {
            sleep_s(5).await;
            1
        };
        let res = timeout(work, Duration::from_secs(10)).await;
        assert!(res == Some(1));
    }

    #[tokio::test]
    async fn timeout_timeout() {
        tokio::time::pause();
        let work = async {
            sleep_s(20).await;
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
            sleep_s(5).await;
        };
        let cancel = async {
            sleep_s(10).await;
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
            sleep_s(20).await;
        };
        let cancel = async {
            sleep_s(10).await;
            token.cancel();
        };
        let cancellable = cancellable(work, &token);
        let (res, _) = join!(cancellable, cancel);
        assert!(res.is_none());
    }
}
