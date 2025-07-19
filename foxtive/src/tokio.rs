use crate::results::AppResult;
use tracing::error;
use std::future::Future;
use std::time::Duration;
use tokio::task::{JoinHandle, spawn_blocking};
use tokio::{spawn, time};

pub struct Tokio;

impl Tokio {
    pub async fn run_blocking<Func, Ret>(func: Func) -> AppResult<Ret>
    where
        Func: FnOnce() -> Ret + Send + 'static,
        Ret: Send + 'static,
    {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        Ok(rt.spawn_blocking(func).await?)
    }

    ///
    ///
    /// # Arguments
    ///
    /// * `interval`: an interval within which the given function will be executed (in milliseconds)
    /// * `func`: The function that will be executed
    ///
    /// returns: ()
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn timeout<Fun, Fut>(interval: u64, func: Fun, name: &str)
    where
        Fun: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        let name = name.to_owned();
        spawn(async move {
            let mut interval = time::interval(Duration::from_millis(interval));

            interval.tick().await;
            interval.tick().await;

            match func().await {
                Ok(_) => {}
                Err(err) => {
                    error!("[execution-error][{name}] {err:?}");
                }
            }
        });
    }

    ///
    ///
    /// # Arguments
    ///
    /// * `interval`: an interval within which the given function will be executed (in milliseconds)
    /// * `func`: the function that will be executed repeatedly
    ///
    /// returns: ()
    ///
    /// # Examples
    ///
    /// ```
    ///
    /// ```
    pub fn tick<Fun, Fut>(interval: u64, func: Fun, name: &str)
    where
        Fun: Fn() -> Fut + Send + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        let name = name.to_owned();
        spawn(async move {
            let mut interval = time::interval(Duration::from_millis(interval));

            loop {
                interval.tick().await;

                match func().await {
                    Ok(_) => {}
                    Err(err) => {
                        error!("[execution-error][{name}] {err:?}");
                    }
                }
            }
        });
    }

    pub fn blk<F, R>(f: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        spawn_blocking(f)
    }
}
