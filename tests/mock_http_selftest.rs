mod support;
use support::mock_http::*;

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
