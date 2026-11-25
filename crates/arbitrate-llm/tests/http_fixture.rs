#[allow(dead_code)]
#[path = "../../../tests/support/mock_http.rs"]
mod mock_http;

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use mock_http::{MockHttp, Reply};
use reqwest::{Client, StatusCode};

fn client() -> Client {
    Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

#[tokio::test]
async fn reads_binary_response_without_authorization() {
    let server = MockHttp::start(vec![Reply::Bytes {
        content_type: "application/octet-stream".into(),
        body: vec![0, 255, 1],
    }]);
    let response = client()
        .get(format!("http://{}/audio", server.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "application/octet-stream"
    );
    assert_eq!(&response.bytes().await.unwrap()[..], [0, 255, 1]);
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(
        !requests[0]
            .headers
            .iter()
            .any(|(name, _)| name == "authorization")
    );
}

#[tokio::test]
async fn truncated_body_is_an_error_after_success_headers() {
    let server = MockHttp::start(vec![Reply::Truncated {
        status: 200,
        prefix: b"data: one".to_vec(),
        missing: 3,
    }]);
    let response = client()
        .get(format!("http://{}/events", server.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let error = response.bytes().await.unwrap_err();
    assert!(!error.is_timeout(), "{error}");
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn paused_sse_yields_the_first_event_before_releasing_eof() {
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let server = MockHttp::start(vec![Reply::PausedSse {
        first: "data: first\n\n".into(),
        release: release.clone(),
    }]);
    let mut response = client()
        .get(format!("http://{}/events", server.addr))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        &response.chunk().await.unwrap().unwrap()[..],
        b"data: first\n\n"
    );
    let next = response.chunk();
    tokio::pin!(next);
    tokio::select! {
        biased;
        result = &mut next => panic!("paused stream completed early: {result:?}"),
        _ = std::future::ready(()) => {}
    }
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    assert!(next.await.unwrap().is_none());
}

#[tokio::test]
async fn openai_preserves_full_endpoints_and_only_sends_nonempty_session() {
    use arbitrate_llm::transport::{
        DEFAULT_REQUEST_TIMEOUT, Transport, TransportClient, user_agent,
    };
    use reqwest::Url;
    use reqwest::header::{HeaderMap, HeaderValue};

    let server = MockHttp::start(vec![
        Reply::Json {
            status: 200,
            body: "{}".into()
        };
        3
    ]);
    let base = Url::parse(&format!("http://{}/inference/go/v1/", server.addr)).unwrap();
    let mut defaults = HeaderMap::new();
    defaults.insert(
        "x-opencode-session",
        HeaderValue::from_static("inherited-fixture-session"),
    );
    defaults.insert("x-fixture-default", HeaderValue::from_static("kept"));
    let injected = TransportClient::new(defaults, false).unwrap();
    let transport = Transport::new(injected, base, DEFAULT_REQUEST_TIMEOUT);
    let default_agent = if cfg!(debug_assertions) {
        "arbitrate/dev"
    } else {
        "arbitrate/0.1.0"
    };
    let endpoints = [
        ("/v1/chat/completions", "", default_agent),
        ("/v1/chat/completions", "test-session", "arbitrate/0.1.0"),
        (
            "/gateway/custom-infer?api-version=test",
            "",
            "arbitrate/dev",
        ),
    ];
    for (index, (target, session, agent)) in endpoints.into_iter().enumerate() {
        let endpoint = Url::parse(&format!("http://{}{target}", server.addr)).unwrap();
        let transport = match index {
            0 => transport.clone(),
            1 => transport.clone().with_user_agent(user_agent(Some("0.1.0"))),
            _ => transport.clone().with_user_agent(user_agent(None)),
        };
        let request = transport
            .openai_request(endpoint, session)
            .body("fixed body");
        assert_eq!(
            request.try_clone().unwrap().build().unwrap().timeout(),
            Some(&Duration::from_secs(300))
        );
        assert_eq!(request.send().await.unwrap().status(), StatusCode::OK);
        let requests = server.requests.lock().unwrap();
        let recorded = &requests[index];
        assert_eq!(recorded.method, "POST");
        assert_eq!(recorded.path, target);
        assert_eq!(&recorded.raw_body, b"fixed body");
        let values = |name| {
            recorded
                .headers
                .iter()
                .filter(|(key, _)| key == name)
                .map(|(_, bytes)| bytes.as_slice())
                .collect::<Vec<_>>()
        };
        assert_eq!(values("user-agent"), [agent.as_bytes()]);
        assert_eq!(values("x-fixture-default"), [b"kept"]);
        let session_headers = values("x-opencode-session");
        if session.is_empty() {
            assert!(session_headers.is_empty());
        } else {
            assert_eq!(session_headers, [session.as_bytes()]);
        }
    }
}

#[test]
fn directory_request_preserves_prefix_and_uses_its_own_timeout() {
    use arbitrate_llm::transport::{
        DEFAULT_CATALOG_TIMEOUT, Transport, TransportClient, user_agent,
    };
    use reqwest::{Method, Url};

    let base = Url::parse("http://127.0.0.1:9/inference/go/v1/").unwrap();
    let client = TransportClient::new(Default::default(), false).unwrap();
    let transport = Transport::new(client.clone(), base.clone(), DEFAULT_CATALOG_TIMEOUT);
    let request = transport
        .request(Method::GET, "/models")
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(request.url().path(), "/inference/go/v1/models");
    assert_eq!(request.timeout(), Some(&Duration::from_secs(5)));
    assert_eq!(user_agent(Some("")), "arbitrate/dev");
    let timeout = Duration::from_millis(30);
    let injected = Transport::new(client, base, timeout);
    assert_eq!(
        injected
            .request(Method::GET, "models")
            .unwrap()
            .build()
            .unwrap()
            .timeout(),
        Some(&timeout)
    );
}
