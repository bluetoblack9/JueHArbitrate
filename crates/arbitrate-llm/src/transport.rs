//! Request construction for generic providers and the model catalogue.

use std::time::Duration;

use reqwest::{Client, Method, RequestBuilder, Url};

pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
pub const DEFAULT_CATALOG_TIMEOUT: Duration = Duration::from_secs(5);
const INVALID_ENDPOINT: &str = "invalid provider endpoint path";

#[derive(Clone)]
pub struct Transport {
    client: Client,
    base: Url,
    timeout: Duration,
    user_agent: String,
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
            user_agent: user_agent(if cfg!(debug_assertions) {
                None
            } else {
                Some(env!("CARGO_PKG_VERSION"))
            }),
        }
    }

    pub fn with_user_agent(mut self, value: String) -> Self {
        self.user_agent = value;
        self
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
        Ok(self
            .client
            .request(method, url)
            .timeout(self.timeout)
            .header(reqwest::header::USER_AGENT, &self.user_agent))
    }

    /// OpenAI-compatible configurations supply the complete POST destination.
    pub fn openai_request(&self, endpoint: Url, session: &str) -> RequestBuilder {
        let request = self
            .client
            .post(endpoint)
            .timeout(self.timeout)
            .header(reqwest::header::USER_AGENT, &self.user_agent);
        if session.is_empty() {
            request
        } else {
            request.header("x-opencode-session", session)
        }
    }
}

pub fn user_agent(version: Option<&str>) -> String {
    format!(
        "arbitrate/{}",
        version.filter(|value| !value.is_empty()).unwrap_or("dev")
    )
}
