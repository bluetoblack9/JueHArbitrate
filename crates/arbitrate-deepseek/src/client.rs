use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use arbitrate_core::types::ChatResponse;
use reqwest::{Body, ClientBuilder, Method, Response, Url, header};
use tokio::sync::watch;

use crate::error::{ApiError, Error};
use crate::retry::retry_call;
use crate::transport::{DEFAULT_REQUEST_TIMEOUT, Transport, wait_retry};
use crate::wire::{ChatRequest, decode_chat, encode_chat};

pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
pub(crate) const ENDPOINT_CHAT: &str = "/chat/completions";
const JSON: &str = "application/json";

type SleepFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

#[derive(Clone)]
pub struct Client {
    api_key: String,
    transport: Transport,
    sleeper: Arc<dyn Fn(Duration) -> SleepFuture + Send + Sync>,
}

impl Client {
    pub fn new(api_key: String) -> Result<Self, Error> {
        let base = Url::parse(DEFAULT_BASE_URL)
            .map_err(|error| Error::RequestConfig(error.to_string()))?;
        Self::with_options(
            api_key,
            base,
            DEFAULT_REQUEST_TIMEOUT,
            reqwest::Client::builder(),
        )
    }

    /// Build a DeepSeek client using `api_key` for Bearer authentication.
    /// The base URL must not contain credentials.
    pub fn with_options(
        api_key: String,
        base: Url,
        timeout: Duration,
        builder: ClientBuilder,
    ) -> Result<Self, Error> {
        if api_key.is_empty() {
            return Err(Error::MissingApiKey);
        }
        if !base.username().is_empty() || base.password().is_some() {
            return Err(Error::RequestConfig(
                "base URL must not contain credentials".into(),
            ));
        }
        let client = builder
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| Error::RequestConfig(error.without_url().to_string()))?;
        Ok(Self {
            api_key,
            transport: Transport::new(client, base, timeout),
            sleeper: Arc::new(|delay| Box::pin(tokio::time::sleep(delay))),
        })
    }

    /// Replace the timer used between attempts. The retry policy supplies the delay.
    pub fn with_sleeper<F, Fut>(mut self, sleeper: F) -> Self
    where
        F: Fn(Duration) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.sleeper = Arc::new(move |delay| Box::pin(sleeper(delay)));
        self
    }

    pub async fn chat(
        &self,
        request: &ChatRequest,
        cancel: &mut watch::Receiver<bool>,
    ) -> Result<ChatResponse, Error> {
        let mut request = request.clone();
        request.stream = false;
        let body = encode_chat(&request).map_err(Error::Encode)?;
        self.retry(cancel, || self.chat_once(&body)).await
    }

    pub(crate) async fn retry<T, Op, Attempt>(
        &self,
        cancel: &mut watch::Receiver<bool>,
        op: Op,
    ) -> Result<T, Error>
    where
        Op: FnMut() -> Attempt,
        Attempt: Future<Output = Result<T, Error>>,
    {
        retry_call(cancel, self.sleeper.as_ref(), op).await
    }

    pub(crate) async fn wait_before_reopen(
        &self,
        cancel: &mut watch::Receiver<bool>,
    ) -> Result<(), Error> {
        wait_retry(cancel, self.sleeper.as_ref())
            .await
            .map_err(|_| Error::Cancelled)
    }

    pub(crate) async fn send_json_once(
        &self,
        endpoint: &str,
        payload: Body,
    ) -> Result<Response, Error> {
        let (client, request) = self
            .transport
            .request(Method::POST, endpoint)
            .map_err(Error::RequestConfig)?
            .bearer_auth(&self.api_key)
            .header(header::CONTENT_TYPE, JSON)
            .header(header::ACCEPT, JSON)
            .body(payload)
            .build_split();
        let request =
            request.map_err(|error| Error::RequestConfig(error.without_url().to_string()))?;
        client
            .execute(request)
            .await
            .map_err(|error| Error::Transport(error.without_url()))
    }

    async fn chat_once(&self, body: &[u8]) -> Result<ChatResponse, Error> {
        let response = self
            .send_json_once(ENDPOINT_CHAT, Body::from(body.to_vec()))
            .await?;
        read_chat_response(response).await
    }
}

async fn read_chat_response(response: Response) -> Result<ChatResponse, Error> {
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|error| Error::Transport(error.without_url()))?;
    if status.is_success() {
        decode_chat(&body).map_err(Error::Decode)
    } else {
        Err(Error::Api(ApiError::from_body(status.as_u16(), &body)))
    }
}

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../tests/support/mock_http.rs"]
mod mock_http;

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::mock_http::{MockHttp, Reply};
    use super::{Client, Error};
    use reqwest::Url;

    fn local_client(server: &MockHttp) -> Client {
        Client::with_options(
            "fixture-key".into(),
            Url::parse(&format!("http://{}", server.addr)).unwrap(),
            Duration::from_secs(5),
            reqwest::Client::builder().no_proxy(),
        )
        .unwrap()
    }

    async fn single_attempt(reply: Reply) -> Error {
        let server = MockHttp::start(vec![reply]);
        let result = local_client(&server).chat_once(b"{}").await.unwrap_err();
        assert_eq!(server.requests.lock().unwrap().len(), 1);
        result
    }

    #[tokio::test]
    async fn single_attempt_preserves_api_envelope_and_fallbacks() {
        let error = single_attempt(Reply::Json {
            status: 400,
            body: r#"{"error":{"type":"invalid_request_error","code":"bad_input","message":"bad input"}}"#.into()
        }).await;
        let Error::Api(error) = error else {
            panic!("{error:?}")
        };
        assert_eq!(error.status, 400);
        assert_eq!(error.kind, "invalid_request_error");
        assert_eq!(error.code, "bad_input");
        assert_eq!(error.message, "bad input");
        assert_eq!(
            error.to_string(),
            "deepseek api error: invalid_request_error: bad input"
        );
        for body in [
            r#"{"error":{"message":""}}"#,
            r#"{"error":["type","code","message"]}"#,
            r#"[[ "type", "code", "message" ]]"#,
            "{",
            "plain failure",
        ] {
            let error = single_attempt(Reply::Json {
                status: 403,
                body: body.into(),
            })
            .await;
            let Error::Api(error) = error else {
                panic!("{error:?}")
            };
            assert_eq!(error.status, 403);
            assert_eq!(error.message, format!("status 403: {body}"));
            assert_eq!(
                error.to_string(),
                format!("deepseek api error: status 403: {body}")
            );
        }
    }

    #[tokio::test]
    async fn single_attempt_classifies_transport_and_decode_errors() {
        for reply in [
            Reply::Hangup,
            Reply::Truncated {
                status: 200,
                prefix: b"{".to_vec(),
                missing: 2,
            },
            Reply::Truncated {
                status: 400,
                prefix: b"{".to_vec(),
                missing: 2,
            },
        ] {
            let error = single_attempt(reply).await;
            let Error::Transport(source) = error else {
                panic!("{error:?}")
            };
            assert!(source.url().is_none());
        }
        let error = single_attempt(Reply::Json {
            status: 200,
            body: "{".into(),
        })
        .await;
        assert!(matches!(error, Error::Decode(_)));
    }

    #[tokio::test]
    async fn retry_cancels_a_pending_response_body_read() {
        use crate::retry::retry_call;
        use std::sync::{Arc, Condvar, Mutex};
        use tokio::sync::watch;

        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let server = MockHttp::start(vec![Reply::PausedSse {
            first: "data: first\n\n".into(),
            release: release.clone(),
        }]);
        let client = local_client(&server);
        let response = tokio::time::timeout(
            Duration::from_secs(2),
            client.send_json_once(super::ENDPOINT_CHAT, reqwest::Body::from("{}")),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(response.status(), 200);
        let mut response = Some(response);
        let mut attempts = 0;
        let (sender, mut cancel) = watch::channel(false);
        {
            let read = retry_call(
                &mut cancel,
                |_| std::future::ready(()),
                || {
                    attempts += 1;
                    super::read_chat_response(response.take().expect("unexpected retry"))
                },
            );
            tokio::pin!(read);
            tokio::select! {
                biased;
                result = &mut read => panic!("body read completed early: {result:?}"),
                _ = std::future::ready(()) => {}
            }
            sender.send_replace(true);
            tokio::select! {
                biased;
                result = &mut read => assert!(matches!(result, Err(Error::Cancelled))),
                _ = std::future::ready(()) => panic!("pending body read did not cancel"),
            }
        }
        *release.0.lock().unwrap() = true;
        release.1.notify_all();
        assert_eq!(attempts, 1);
        assert_eq!(server.requests.lock().unwrap().len(), 1);
    }
}
