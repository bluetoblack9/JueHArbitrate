use std::future::Future;
use std::time::Duration;

use tokio::sync::watch;

use crate::error::Error;
use crate::transport::{until_cancelled, wait_retry};

/// Retry a single endpoint operation. Stream pumping owns a separate budget.
pub(crate) async fn retry_call<T, Op, Attempt, Sleep, Delay>(
    cancel: &mut watch::Receiver<bool>,
    sleep: Sleep,
    mut op: Op,
) -> Result<T, Error>
where
    Op: FnMut() -> Attempt,
    Attempt: Future<Output = Result<T, Error>>,
    Sleep: FnOnce(Duration) -> Delay,
    Delay: Future<Output = ()>,
{
    match until_cancelled(cancel, async { op().await }).await {
        Ok(Err(error)) if retryable(&error) => {}
        Ok(result) => return result,
        Err(_) => return Err(Error::Cancelled),
    }
    wait_retry(cancel, sleep)
        .await
        .map_err(|_| Error::Cancelled)?;

    match until_cancelled(cancel, async { op().await }).await {
        Ok(Err(error)) if retryable(&error) => Err(Error::Exhausted {
            attempts: 2,
            source: Box::new(error),
        }),
        Ok(result) => result,
        Err(_) => Err(Error::Cancelled),
    }
}

fn retryable(error: &Error) -> bool {
    match error {
        Error::Api(error) => error.status == 429 || (500..=599).contains(&error.status),
        Error::Transport(_) | Error::Decode(_) => true,
        _ => false,
    }
}
