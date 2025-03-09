use crate::tokio::Tokio;
use tokio::task::JoinHandle;

pub fn blk<F, R>(f: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    Tokio::blk(f)
}
