#[allow(dead_code)]
#[path = "../../../tests/support/provider_messages.rs"]
mod provider_messages;

use arbitrate_core::types::Message;
use arbitrate_deepseek::strip_reasoning_content;

#[test]
fn strip_preserves_only_tool_call_reasoning_and_keeps_input() {
    let sample = provider_messages::sample_turn();
    let mut with_calls = sample[1].clone();
    with_calls.tool_calls.push(with_calls.tool_calls[0].clone());
    let mut history = vec![with_calls];
    for role in ["assistant", "user", "system", "tool"] {
        history.push(Message {
            role: role.into(),
            reasoning_content: format!("reasoning-{role}"),
            predicted_next: format!("prediction-{role}"),
            ..Message::default()
        });
    }
    history[4].tool_call_id = sample[2].tool_call_id.clone();
    let original = history.clone();
    let prepared = strip_reasoning_content(&history);

    assert_eq!(history, original);
    assert_eq!(prepared[0].reasoning_content, original[0].reasoning_content);
    assert_eq!(prepared[0].tool_calls, original[0].tool_calls);
    assert_eq!(prepared[4].tool_call_id, prepared[0].tool_calls[0].id);
    assert!(prepared[1..].iter().all(|m| m.reasoning_content.is_empty()));
    assert!(prepared.iter().all(|m| m.predicted_next.is_empty()));
}

#[allow(dead_code)]
#[path = "../../../tests/support/mock_http.rs"]
mod mock_http;

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use arbitrate_core::types::{ToolCall, Usage};
use arbitrate_deepseek::wire::{ChatRequest, StreamOptions};
use arbitrate_deepseek::{Client, Error, StreamEvent};
use mock_http::{MockHttp, Reply};
use tokio::sync::{mpsc, watch};
use tokio::time::timeout;

const TEST_LIMIT: Duration = Duration::from_secs(2);

fn request() -> ChatRequest {
    ChatRequest {
        model: "deepseek-flash".into(),
        ..Default::default()
    }
}

fn client(server: &MockHttp, request_timeout: Duration) -> Client {
    Client::with_options(
        "fixture-key".into(),
        reqwest::Url::parse(&format!("http://{}/proxy", server.addr)).unwrap(),
        request_timeout,
        reqwest::Client::builder().no_proxy(),
    )
    .unwrap()
}

fn fast_client(server: &MockHttp) -> Client {
    client(server, Duration::from_secs(20)).with_sleeper(|_| std::future::ready(()))
}

fn sse(body: &str) -> Reply {
    Reply::Sse {
        chunks: vec![body.into()],
        delay_ms: 0,
    }
}

fn json(status: u16) -> Reply {
    Reply::Json {
        status,
        body: "{}".into(),
    }
}

fn truncated(status: u16, prefix: &str) -> Reply {
    Reply::Truncated {
        status,
        prefix: prefix.as_bytes().to_vec(),
        missing: 32,
    }
}

fn delta(text: &str) -> String {
    let chunk = serde_json::json!({"choices":[{"delta":{"content":text}}]});
    format!("data: {chunk}\n\n")
}

const EMPTY: &str = "data: [DONE]\n\n";
const NORMAL: &str = concat!(
    "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\n",
    "data: [DONE]\n\n",
);
const BAD_JSON: &str = "data: {broken\n\n";

fn done(finish_reason: &str) -> StreamEvent {
    StreamEvent::Done {
        usage: Usage::default(),
        finish_reason: finish_reason.into(),
    }
}

async fn drain(mut stream: mpsc::Receiver<StreamEvent>) -> Vec<StreamEvent> {
    timeout(TEST_LIMIT, async {
        let mut events = Vec::new();
        while let Some(event) = stream.recv().await {
            events.push(event);
        }
        events
    })
    .await
    .expect("event channel did not close")
}

async fn run(server: &MockHttp) -> Result<Vec<StreamEvent>, Error> {
    let client = fast_client(server);
    let (_keep_open, cancel) = watch::channel(false);
    let stream = timeout(TEST_LIMIT, client.chat_stream(&request(), cancel))
        .await
        .expect("chat_stream did not return")?;
    Ok(drain(stream).await)
}

fn assert_count(server: &MockHttp, expected: usize) {
    assert_eq!(server.requests.lock().unwrap().len(), expected);
}

#[tokio::test]
async fn open_and_pump_retry_budgets_are_independent() {
    let server = MockHttp::start(vec![json(503), sse(EMPTY), json(503), sse(NORMAL)]);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let waits = observed.clone();
    let client = client(&server, Duration::from_secs(20)).with_sleeper(move |duration| {
        waits.lock().unwrap().push(duration);
        std::future::ready(())
    });
    let (_keep_open, cancel) = watch::channel(false);
    let stream = timeout(TEST_LIMIT, client.chat_stream(&request(), cancel))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        drain(stream).await,
        [StreamEvent::Delta("ok".into()), done("stop")]
    );

    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    assert!(
        requests
            .iter()
            .all(|r| r.method == "POST" && r.path == "/proxy/chat/completions")
    );
    let sent: serde_json::Value = serde_json::from_slice(&requests[0].raw_body).unwrap();
    assert_eq!(sent["stream"].as_bool(), Some(true));
    assert_eq!(
        sent["stream_options"]["include_usage"].as_bool(),
        Some(true)
    );
    assert!(
        requests
            .windows(2)
            .all(|pair| pair[0].raw_body == pair[1].raw_body)
    );
    assert_eq!(
        *observed.lock().unwrap(),
        vec![Duration::from_millis(500); 3]
    );
}

#[tokio::test]
async fn headers_open_stream_before_first_delta() {
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"first\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let server = MockHttp::start(vec![Reply::SseAfterRelease {
        release: release.clone(),
        body: body.into(),
    }]);
    let client = fast_client(&server);
    let input = ChatRequest {
        stream_options: Some(StreamOptions {
            include_usage: false,
        }),
        ..request()
    };
    let (_keep_open, cancel) = watch::channel(false);
    let mut stream = timeout(TEST_LIMIT, client.chat_stream(&input, cancel))
        .await
        .expect("stream waited for its first body fragment")
        .unwrap();
    assert_eq!(stream.try_recv(), Err(mpsc::error::TryRecvError::Empty));
    {
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let sent: serde_json::Value = serde_json::from_slice(&requests[0].raw_body).unwrap();
        assert_eq!(sent["stream"].as_bool(), Some(true));
        assert_eq!(
            sent["stream_options"]["include_usage"].as_bool(),
            Some(false)
        );
    }
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    assert_eq!(
        drain(stream).await,
        [StreamEvent::Delta("first".into()), done("stop")]
    );
    assert_count(&server, 1);
}

#[tokio::test]
async fn initial_open_preserves_permanent_status_and_retries_transient_failures() {
    let server = MockHttp::start(vec![json(503), json(502)]);
    assert!(matches!(run(&server).await,
        Err(Error::Exhausted { attempts: 2, source })
            if matches!(source.as_ref(), Error::Api(error) if error.status == 502)));
    assert_count(&server, 2);

    for initial in [json(429), Reply::Hangup, truncated(503, "{")] {
        let server = MockHttp::start(vec![initial, sse(NORMAL)]);
        assert_eq!(
            run(&server).await.unwrap(),
            [StreamEvent::Delta("ok".into()), done("stop")]
        );
        assert_count(&server, 2);
    }

    for initial in [json(401), truncated(401, "{")] {
        let incomplete = matches!(initial, Reply::Truncated { .. });
        let server = MockHttp::start(vec![initial]);
        let error = run(&server).await.unwrap_err();
        assert!(!error.to_string().contains("(after"));
        let Error::Api(error) = error else {
            panic!("expected HTTP status, got {error}")
        };
        assert_eq!(error.status, 401);
        if incomplete {
            assert_eq!(error.message, "status 401: incomplete response body");
        }
        assert_count(&server, 1);
    }

    let server = MockHttp::start(vec![truncated(503, "{"), truncated(503, "{")]);
    assert!(matches!(run(&server).await,
        Err(Error::Exhausted { attempts: 2, source })
            if matches!(source.as_ref(), Error::Transport(error) if error.url().is_none())));
    assert_count(&server, 2);
}

#[tokio::test]
async fn eventless_stream_can_reopen_once_and_keeps_the_successful_usage() {
    let final_stream = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}],\"usage\":{\"total_tokens\":8}}\n\n",
        "data: [DONE]\n\n",
    );
    let oversized = format!("data: {}\n", " ".repeat(1024 * 1024));
    for first in [
        sse(EMPTY),
        sse(BAD_JSON),
        sse(&oversized),
        truncated(200, ""),
        sse(
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}],\"usage\":{\"total_tokens\":99}}\n\n",
        ),
    ] {
        let server = MockHttp::start(vec![first, sse(final_stream)]);
        assert_eq!(
            run(&server).await.unwrap(),
            [
                StreamEvent::Delta("ok".into()),
                StreamEvent::Done {
                    usage: Usage {
                        total_tokens: 8,
                        ..Default::default()
                    },
                    finish_reason: "stop".into()
                },
            ]
        );
        assert_count(&server, 2);
    }

    let server = MockHttp::start(vec![sse(EMPTY), sse(EMPTY)]);
    assert_eq!(run(&server).await.unwrap(), [done("")]);
    assert_count(&server, 2);

    let server = MockHttp::start(vec![sse(
        "data: {\"choices\":[{\"finish_reason\":\"stop\"}]}\n",
    )]);
    assert_eq!(run(&server).await.unwrap(), [done("stop")]);
    assert_count(&server, 1);
}

#[tokio::test]
async fn every_emitted_delta_kind_prevents_reopen_after_a_failure() {
    for (delta, expected) in [
        (
            r#"{"reasoning_content":"r"}"#,
            StreamEvent::ReasoningDelta("r".into()),
        ),
        (r#"{"content":"c"}"#, StreamEvent::Delta("c".into())),
        (
            r#"{"tool_calls":[{"index":1,"function":{"arguments":"{"}}]}"#,
            StreamEvent::ToolCallDelta(ToolCall {
                index: Some(1),
                arguments: "{".into(),
                ..Default::default()
            }),
        ),
    ] {
        let first = format!(
            "data: {{\"choices\":[{{\"delta\":{delta}}}],\"usage\":{{\"total_tokens\":7}}}}\n\n"
        );
        for (reply, prefix) in [
            (sse(&format!("{first}{BAD_JSON}")), "decode_error:"),
            (truncated(200, &first), "stream_error:"),
        ] {
            let server = MockHttp::start(vec![reply]);
            let events = run(&server).await.unwrap();
            assert_eq!(events.len(), 2);
            assert_eq!(events[0], expected);
            assert!(
                matches!(&events[1], StreamEvent::Done { usage, finish_reason }
                if usage.total_tokens == 7 && finish_reason.starts_with(prefix))
            );
            assert_count(&server, 1);
        }
    }
}

#[tokio::test]
async fn reopen_failure_terminates_the_channel_using_the_first_usage() {
    let first = sse("data: {\"usage\":{\"total_tokens\":7}}\n\ndata: [DONE]\n\n");
    for (statuses, status, attempts) in [(vec![503, 502], 502, true), (vec![401], 401, false)] {
        let server = MockHttp::start(
            std::iter::once(first.clone())
                .chain(statuses.into_iter().map(json))
                .collect(),
        );
        let events = run(&server).await.unwrap();
        let [
            StreamEvent::Done {
                usage,
                finish_reason,
            },
        ] = events.as_slice()
        else {
            panic!("{events:?}")
        };
        assert_eq!(usage.total_tokens, 7);
        assert!(finish_reason.starts_with(&format!(
            "stream_error:deepseek api error: status {status}:"
        )));
        assert_eq!(finish_reason.ends_with("(after 2 attempts)"), attempts);
        assert_count(&server, if attempts { 3 } else { 2 });
    }
}

async fn wait_until(mut predicate: impl FnMut() -> bool) {
    timeout(TEST_LIMIT, async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("fixture did not reach the test state");
}

#[tokio::test]
async fn cancellation_before_and_during_open_ends_without_retry() {
    let server = MockHttp::start(vec![]);
    let client = fast_client(&server);
    let (_keep_open, cancel) = watch::channel(true);
    assert!(matches!(
        client.chat_stream(&request(), cancel).await,
        Err(Error::Cancelled)
    ));
    assert_count(&server, 0);

    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let server = MockHttp::start(vec![Reply::PausedHeaders {
        release: release.clone(),
    }]);
    let client = fast_client(&server);
    let (cancel, receiver) = watch::channel(false);
    let opening = tokio::spawn(async move { client.chat_stream(&request(), receiver).await });
    wait_until(|| server.requests.lock().unwrap().len() == 1).await;
    cancel.send(true).unwrap();
    assert!(matches!(
        timeout(TEST_LIMIT, opening).await.unwrap().unwrap(),
        Err(Error::Cancelled)
    ));
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    assert_count(&server, 1);
}

#[tokio::test]
async fn cancellation_and_receiver_drop_close_the_http_connection() {
    for (cancel_stream, fill_buffer) in [(true, false), (true, true), (false, false)] {
        let (disconnected, closed) = std::sync::mpsc::channel();
        let (body_sent, written) = std::sync::mpsc::channel();
        let count = if fill_buffer { 17 } else { 1 };
        let server = MockHttp::start(vec![Reply::SseUntilDisconnect {
            chunks: vec![(0..count).map(|index| delta(&index.to_string())).collect()],
            body_sent: Some(body_sent),
            disconnected,
        }]);
        let client = fast_client(&server);
        let (cancel, receiver) = watch::channel(false);
        let mut stream = timeout(TEST_LIMIT, client.chat_stream(&request(), receiver))
            .await
            .unwrap()
            .unwrap();
        timeout(
            TEST_LIMIT,
            tokio::task::spawn_blocking(move || written.recv_timeout(TEST_LIMIT)),
        )
        .await
        .unwrap()
        .unwrap()
        .expect("fixture did not send the SSE data");
        if fill_buffer {
            wait_until(|| stream.len() == 16).await;
        } else {
            assert_eq!(
                timeout(TEST_LIMIT, stream.recv()).await.unwrap(),
                Some(StreamEvent::Delta("0".into()))
            );
        }
        if cancel_stream {
            cancel.send(true).unwrap();
        } else {
            drop(stream);
            timeout(
                TEST_LIMIT,
                tokio::task::spawn_blocking(move || closed.recv_timeout(TEST_LIMIT)),
            )
            .await
            .unwrap()
            .unwrap()
            .expect("HTTP body survived receiver drop");
            assert_count(&server, 1);
            continue;
        }
        timeout(
            TEST_LIMIT,
            tokio::task::spawn_blocking(move || closed.recv_timeout(TEST_LIMIT)),
        )
        .await
        .unwrap()
        .unwrap()
        .expect("HTTP body survived cancellation");
        let pending = drain(stream).await;
        assert_eq!(pending.len(), if fill_buffer { 16 } else { 0 });
        assert!(
            pending
                .iter()
                .all(|event| matches!(event, StreamEvent::Delta(_)))
        );
        assert_count(&server, 1);
    }
}

fn clocked_client(
    server: &MockHttp,
) -> (
    Client,
    watch::Sender<Duration>,
    mpsc::UnboundedReceiver<Duration>,
) {
    let (clock, time) = watch::channel(Duration::ZERO);
    let (updates, observed) = mpsc::unbounded_channel();
    let client = client(server, Duration::from_secs(20)).with_sleeper(move |delay| {
        let mut time = time.clone();
        let updates = updates.clone();
        async move {
            let mut elapsed = *time.borrow_and_update();
            while elapsed < delay {
                updates.send(elapsed).unwrap();
                time.changed().await.unwrap();
                elapsed = *time.borrow_and_update();
            }
        }
    });
    (client, clock, observed)
}

#[tokio::test]
async fn each_backoff_waits_until_500ms_and_can_be_cancelled_at_zero() {
    for (first, cancel_at_zero) in [
        (json(503), true),
        (sse(EMPTY), true),
        (json(503), false),
        (sse(EMPTY), false),
    ] {
        let opened_before_backoff = matches!(&first, Reply::Sse { .. });
        let server = MockHttp::start(vec![first, sse(NORMAL)]);
        let (client, clock, mut observed) = clocked_client(&server);
        let (cancel, receiver) = watch::channel(false);
        let opening = tokio::spawn(async move {
            match client.chat_stream(&request(), receiver).await {
                Ok(stream) => Ok(drain(stream).await),
                Err(error) => Err(error),
            }
        });
        assert_eq!(
            timeout(TEST_LIMIT, observed.recv()).await.unwrap(),
            Some(Duration::ZERO)
        );
        assert_count(&server, 1);
        if cancel_at_zero {
            cancel.send(true).unwrap();
            let result = timeout(TEST_LIMIT, opening).await.unwrap().unwrap();
            if opened_before_backoff {
                assert!(result.unwrap().is_empty());
            } else {
                assert!(matches!(result, Err(Error::Cancelled)));
            }
            clock.send_replace(Duration::from_millis(500));
            assert_count(&server, 1);
        } else {
            clock.send_replace(Duration::from_millis(499));
            assert_eq!(
                timeout(TEST_LIMIT, observed.recv()).await.unwrap(),
                Some(Duration::from_millis(499))
            );
            assert_count(&server, 1);
            clock.send_replace(Duration::from_millis(500));
            assert_eq!(
                timeout(TEST_LIMIT, opening)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap(),
                [StreamEvent::Delta("ok".into()), done("stop")]
            );
            assert_count(&server, 2);
        }
    }
}

#[tokio::test]
async fn transport_timeout_has_distinct_open_and_post_delta_outcomes() {
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let server = MockHttp::start(vec![
        Reply::PausedHeaders {
            release: release.clone()
        };
        2
    ]);
    let client = client(&server, Duration::from_secs(1)).with_sleeper(|_| std::future::ready(()));
    let (_keep_open, cancel) = watch::channel(false);
    let result = timeout(
        Duration::from_secs(4),
        client.chat_stream(&request(), cancel),
    )
    .await
    .unwrap();
    assert!(
        matches!(result, Err(Error::Exhausted { attempts: 2, source })
        if matches!(source.as_ref(), Error::Transport(error) if error.is_timeout()))
    );
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    assert_count(&server, 2);

    let (disconnected, _closed) = std::sync::mpsc::channel();
    let server = MockHttp::start(vec![Reply::SseUntilDisconnect {
        chunks: vec![delta("first")],
        body_sent: None,
        disconnected,
    }]);
    let client = Client::with_options(
        "fixture-key".into(),
        reqwest::Url::parse(&format!("http://{}", server.addr)).unwrap(),
        Duration::from_secs(1),
        reqwest::Client::builder().no_proxy(),
    )
    .unwrap();
    let (_keep_open, cancel) = watch::channel(false);
    let events = timeout(Duration::from_secs(4), async {
        let stream = client.chat_stream(&request(), cancel).await.unwrap();
        drain(stream).await
    })
    .await
    .unwrap();
    let [
        StreamEvent::Delta(text),
        StreamEvent::Done { finish_reason, .. },
    ] = events.as_slice()
    else {
        panic!("expected increment then timeout, got {events:?}");
    };
    assert_eq!(text, "first");
    assert!(finish_reason.starts_with("stream_error:"));
    assert_count(&server, 1);
}

#[tokio::test]
async fn tool_rounds_replay_only_reasoning_tied_to_calls() {
    let tool_stream = |number: usize| {
        sse(&format!(
            "data: {{\"choices\":[{{\"delta\":{{\"reasoning_content\":\"r{number}\",\"tool_calls\":[{{\"id\":\"call-{number}\",\"type\":\"function\",\"function\":{{\"name\":\"lookup\",\"arguments\":\"{{\"}}}}]}}}}]}}\n\n\
         data: {{\"choices\":[{{\"delta\":{{\"tool_calls\":[{{\"index\":0,\"function\":{{\"arguments\":\"}}\"}}}}]}},\"finish_reason\":\"tool_calls\"}}]}}\n\n{EMPTY}"
        ))
    };
    let server = MockHttp::start(vec![tool_stream(1), tool_stream(2), sse(NORMAL)]);
    let client = fast_client(&server);
    let (keep_open, cancel) = watch::channel(false);
    let mut history = vec![
        Message {
            role: "assistant".into(),
            reasoning_content: "standalone".into(),
            predicted_next: "maybe".into(),
            ..Default::default()
        },
        Message {
            role: "user".into(),
            content: "look it up".into(),
            ..Default::default()
        },
    ];

    for round in 1..=3 {
        let original = history.clone();
        let input = ChatRequest {
            messages: strip_reasoning_content(&history),
            ..request()
        };
        let stream = timeout(TEST_LIMIT, client.chat_stream(&input, cancel.clone()))
            .await
            .unwrap()
            .unwrap();
        let events = drain(stream).await;
        assert_eq!(history, original);
        let requests = server.requests.lock().unwrap();
        let body: serde_json::Value =
            serde_json::from_slice(&requests[round - 1].raw_body).unwrap();
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2 * round);
        assert!(messages[0].get("reasoning_content").is_none());
        assert!(
            messages
                .iter()
                .all(|message| message.get("predicted_next").is_none())
        );
        for previous in 1..round {
            let assistant = &messages[2 * previous];
            let tool = &messages[2 * previous + 1];
            assert_eq!(assistant["reasoning_content"], format!("r{previous}"));
            assert_eq!(assistant["tool_calls"][0]["id"], format!("call-{previous}"));
            assert_eq!(assistant["tool_calls"][0]["function"]["arguments"], "{}");
            assert_eq!(tool["tool_call_id"], format!("call-{previous}"));
        }
        if round == 3 {
            assert_eq!(events, [StreamEvent::Delta("ok".into()), done("stop")]);
            break;
        }
        let [
            StreamEvent::ReasoningDelta(reasoning),
            StreamEvent::ToolCallDelta(start),
            StreamEvent::ToolCallDelta(end),
            terminal,
        ] = events.as_slice()
        else {
            panic!("invalid tool stream: {events:?}");
        };
        assert_eq!(start.index, Some(0));
        assert_eq!(end.index, Some(0));
        assert_eq!(*terminal, done("tool_calls"));
        let mut call = start.clone();
        call.arguments.push_str(&end.arguments);
        history.push(Message {
            role: "assistant".into(),
            reasoning_content: reasoning.clone(),
            tool_calls: vec![call.clone()],
            predicted_next: "later".into(),
            ..Default::default()
        });
        history.push(Message {
            role: "tool".into(),
            tool_call_id: call.id,
            content: "result".into(),
            ..Default::default()
        });
    }
    drop(keep_open);
    assert_count(&server, 3);
}
