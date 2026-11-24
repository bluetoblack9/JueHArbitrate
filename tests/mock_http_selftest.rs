mod support;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::time::Duration;
use support::mock_http::*;

#[test]
fn connected_client_can_wait_before_sending_request() {
    let m = MockHttp::start(vec![Reply::Json {
        status: 200,
        body: "delayed-request-ok".into(),
    }]);
    let mut client = TcpStream::connect(m.addr).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();

    // No request bytes exist yet: the server must wait, keeping the connection open.
    let idle_result = client.peek(&mut [0u8; 1]);
    assert!(
        matches!(
            idle_result,
            Err(ref error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut)
        ),
        "the server must await request headers, got {idle_result:?}"
    );

    client
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    client
        .write_all(b"POST /delayed HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\n\r\n{}")
        .unwrap();
    let mut response = String::new();
    client.read_to_string(&mut response).unwrap();
    assert_eq!(
        response,
        "HTTP/1.1 200 X\r\nContent-Type: application/json\r\nContent-Length: 18\r\nConnection: close\r\n\r\ndelayed-request-ok"
    );
    let requests = m.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/delayed");
    assert_eq!(requests[0].body, "{}");
}

#[test]
fn json_roundtrip_and_request_recording() {
    let m = MockHttp::start(vec![Reply::Json {
        status: 200,
        body: r#"{"ok":true}"#.into(),
    }]);
    let (status, body) =
        http_request(m.addr, "POST", "/chat/completions", r#"{"model":"x"}"#).unwrap();
    assert_eq!(status, 200);
    assert_eq!(body, r#"{"ok":true}"#);
    let reqs = m.requests.lock().unwrap();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].method, "POST");
    assert_eq!(reqs[0].path, "/chat/completions");
    assert_eq!(reqs[0].body, r#"{"model":"x"}"#);
}

#[test]
fn sse_fragmented_chunks_and_done() {
    let m = MockHttp::start(vec![Reply::Sse {
        chunks: vec![
            "data: {\"delta\":\"a\"}\n\n".into(),
            "data: [DONE]\n\n".into(),
        ],
        delay_ms: 1,
    }]);
    let (status, raw) = http_request(m.addr, "GET", "/events", "").unwrap();
    assert_eq!(status, 200);
    let data = dechunk(&raw);
    let ev = sse_events(&data);
    assert_eq!(ev.len(), 2);
    assert!(ev[1].contains("[DONE]"));
}

#[test]
fn sse_coalesced_events_split() {
    let m = MockHttp::start(vec![Reply::Sse {
        chunks: vec!["data: one\n\ndata: two\n\n".into()],
        delay_ms: 0,
    }]);
    let (_, raw) = http_request(m.addr, "GET", "/events", "").unwrap();
    assert_eq!(sse_events(&dechunk(&raw)).len(), 2, "粘包必须可拆分");
}

#[test]
fn hangup_then_retry_succeeds() {
    let m = MockHttp::start(vec![
        Reply::Hangup,
        Reply::Json {
            status: 200,
            body: "retry-ok".into(),
        },
    ]);
    assert!(
        http_request(m.addr, "GET", "/x", "").is_err(),
        "首次连接应断开"
    );
    let (status, body) = http_request(m.addr, "GET", "/x", "").unwrap();
    assert_eq!((status, body.as_str()), (200, "retry-ok"));
}

#[test]
fn usage_cache_metadata_body_intact() {
    let body = r#"{"usage":{"prompt_tokens":10,"prompt_cache_hit_tokens":8,"prompt_cache_miss_tokens":2}}"#;
    let m = MockHttp::start(vec![Reply::Json {
        status: 200,
        body: body.into(),
    }]);
    let (_, got) = http_request(m.addr, "POST", "/chat", "").unwrap();
    let v: serde_json::Value = serde_json::from_str(&got).unwrap();
    assert_eq!(v["usage"]["prompt_cache_hit_tokens"], 8);
}
