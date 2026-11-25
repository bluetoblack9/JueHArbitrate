//! Request construction for DeepSeek endpoints.

use std::future::Future;
use std::time::Duration;

use reqwest::{Client, Method, RequestBuilder, Url};
use tokio::sync::watch;

pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
const INVALID_ENDPOINT: &str = "invalid provider endpoint path";

pub struct Transport {
    client: Client,
    base: Url,
    timeout: Duration,
}

impl Transport {
    pub fn new(client: Client, mut base: Url, timeout: Duration) -> Self {
        if !base.path().ends_with('/')
            && let Ok(mut segments) = base.path_segments_mut()
        {
            segments.push("");
        }
        Self {
            client,
            base,
            timeout,
        }
    }

    /// Build a request below the base directory. Callers add query parameters separately.
    pub fn request(&self, method: Method, relative: &str) -> Result<RequestBuilder, String> {
        if relative.trim() != relative
            || relative.starts_with("//")
            || relative.contains(['?', '#', '\\'])
            || relative.chars().any(char::is_control)
        {
            return Err(INVALID_ENDPOINT.into());
        }
        let relative = relative.strip_prefix('/').unwrap_or(relative);
        let url = self
            .base
            .join(relative)
            .map_err(|_| INVALID_ENDPOINT.to_owned())?;
        if !matches!(url.scheme(), "http" | "https")
            || url.origin() != self.base.origin()
            || relative
                .split('/')
                .next()
                .is_some_and(|segment| segment.contains(':'))
            || !url.path().starts_with(self.base.path())
        {
            return Err(INVALID_ENDPOINT.into());
        }
        Ok(self.client.request(method, url).timeout(self.timeout))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

pub const RETRY_DELAY: Duration = Duration::from_millis(500);

/// Use a caller-supplied timer with the provider's retry delay.
pub async fn wait_retry<S, F>(cancel: &mut watch::Receiver<bool>, sleep: S) -> Result<(), Cancelled>
where
    S: FnOnce(Duration) -> F,
    F: Future<Output = ()>,
{
    until_cancelled(cancel, sleep(RETRY_DELAY)).await
}

/// Stop an operation when cancellation arrives or its control sender closes.
pub async fn until_cancelled<F: Future>(
    cancel: &mut watch::Receiver<bool>,
    operation: F,
) -> Result<F::Output, Cancelled> {
    tokio::select! {
        biased;
        _ = cancel.wait_for(|value| *value) => Err(Cancelled),
        result = operation => Ok(result),
    }
}
