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
