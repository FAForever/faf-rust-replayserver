use async_trait::async_trait;

// Fn would be nicer, but async is hard.
#[async_trait(?Send)]
pub trait Consumer<F, T> {
    async fn consume(&self, f: F) -> T;
}
