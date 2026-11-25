use std::time::Duration;

use arbitrate_core::types::ChatResponse;
use reqwest::{Body, ClientBuilder, Method, Response, Url, header};
use tokio::sync::watch;

use crate::error::{ApiError, Error};
use crate::transport::{DEFAULT_REQUEST_TIMEOUT, Transport, until_cancelled};
use crate::wire::{ChatRequest, decode_chat, encode_chat};

pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
pub(crate) const ENDPOINT_CHAT: &str = "/chat/completions";
const JSON: &str = "application/json";

pub struct Client {
    api_key: String,
    transport: Transport,
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

    pub fn with_options(
        api_key: String,
        base: Url,
        timeout: Duration,
        builder: ClientBuilder,
    ) -> Result<Self, Error> {
        if api_key.is_empty() {
            return Err(Error::MissingApiKey);
        }
        let client = builder
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| Error::RequestConfig(error.without_url().to_string()))?;
        Ok(Self {
            api_key,
            transport: Transport::new(client, base, timeout),
        })
    }

    pub async fn chat(
        &self,
        request: &ChatRequest,
        cancel: &mut watch::Receiver<bool>,
    ) -> Result<ChatResponse, Error> {
        let mut request = request.clone();
        request.stream = false;
        let body = encode_chat(&request).map_err(Error::Encode)?;
        until_cancelled(cancel, self.chat_once(&body))
            .await
            .map_err(|_| Error::Cancelled)?
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
        for body in [r#"{"error":{"message":""}}"#, "{", "plain failure"] {
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
}
