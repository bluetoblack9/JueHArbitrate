#[allow(dead_code)]
#[path = "../../../tests/support/mock_http.rs"]
mod mock_http;

use std::time::Duration;

use arbitrate_core::types::Message;
use arbitrate_deepseek::wire::{ChatRequest, EncodeError, MODEL_V41_FLASH};
use arbitrate_deepseek::{Client, Error};
use mock_http::{MockHttp, Reply};
use reqwest::Url;
use reqwest::header::{HeaderMap, HeaderValue};
use tokio::sync::watch;

fn request() -> ChatRequest {
    ChatRequest {
        model: MODEL_V41_FLASH.into(),
        messages: vec![Message {
            role: "user".into(),
            content: "hi".into(),
            ..Message::default()
        }],
        stream: true,
        ..ChatRequest::default()
    }
}

fn client(server: &MockHttp, builder: reqwest::ClientBuilder) -> Client {
    Client::with_options(
        "fixture-key".into(),
        Url::parse(&format!("http://{}/proxy", server.addr)).unwrap(),
        Duration::from_secs(5),
        builder,
    )
    .unwrap()
}

fn success() -> Reply {
    Reply::Json {
        status: 200,
        body: r#"{"choices":[{"index":0,"message":{"role":"assistant","content":"one"}},{"index":1,"message":{"content":"two"}}],"usage":{"prompt_tokens":3,"prompt_cache_hit_tokens":2,"prompt_cache_miss_tokens":1}}"#.into(),
    }
}

#[tokio::test]
async fn chat_uses_injected_client_and_keeps_original_stream_flag() {
    let server = MockHttp::start(vec![success()]);
    let mut headers = HeaderMap::new();
    headers.insert("X-Fixture-Client", HeaderValue::from_static("injected"));
    let client = client(
        &server,
        reqwest::Client::builder()
            .no_proxy()
            .default_headers(headers),
    );
    let input = request();
    let (_sender, mut cancel) = watch::channel(false);
    let response = client.chat(&input, &mut cancel).await.unwrap();
    assert!(input.stream);
    assert_eq!(response.choices[0].message.content, "one");
    assert_eq!(response.choices[1].message.content, "two");
    assert_eq!(response.usage.prompt_tokens, 3);
    assert_eq!(response.usage.prompt_cache_hit_tokens, 2);
    assert_eq!(response.usage.prompt_cache_miss_tokens, 1);

    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let sent = &requests[0];
    assert_eq!(sent.method, "POST");
    assert_eq!(sent.path, "/proxy/chat/completions");
    for (name, value) in [
        ("authorization", "Bearer fixture-key"),
        ("content-type", "application/json"),
        ("accept", "application/json"),
        ("x-fixture-client", "injected"),
    ] {
        let values: Vec<_> = sent
            .headers
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, bytes)| bytes.as_slice())
            .collect();
        assert_eq!(values, [value.as_bytes()], "{name}");
    }
    assert_eq!(
        sent.raw_body,
        br#"{"model":"deepseek-flash","messages":[{"role":"user","content":"hi"}]}"#
    );
}

#[tokio::test]
async fn invalid_input_and_preexisting_cancellation_send_no_request() {
    assert!(matches!(
        Client::new(String::new()),
        Err(Error::MissingApiKey)
    ));
    let Err(error) = Client::new(String::new()) else {
        panic!("missing key accepted")
    };
    assert!(error.to_string().contains("DEEPSEEK_API_KEY"));
    let server = MockHttp::start(vec![]);
    let client = client(&server, reqwest::Client::builder().no_proxy());
    let (_sender, mut cancel) = watch::channel(false);
    let mut input = request();
    input.messages[0].images.push(Default::default());
    assert!(matches!(
        client.chat(&input, &mut cancel).await,
        Err(Error::Encode(EncodeError::UnsupportedAttachments))
    ));
    input.messages[0].images.clear();
    input.top_p = Some(f64::INFINITY);
    assert!(matches!(
        client.chat(&input, &mut cancel).await,
        Err(Error::Encode(EncodeError::NonFinite("top_p")))
    ));
    let (_sender, mut cancel) = watch::channel(true);
    assert!(matches!(
        client.chat(&request(), &mut cancel).await,
        Err(Error::Cancelled)
    ));
    assert!(server.requests.lock().unwrap().is_empty());
}
