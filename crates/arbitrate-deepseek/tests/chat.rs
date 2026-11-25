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

fn json_reply(status: u16, body: &str) -> Reply {
    Reply::Json {
        status,
        body: body.into(),
    }
}

fn truncated_reply(status: u16) -> Reply {
    Reply::Truncated {
        status,
        prefix: b"{".to_vec(),
        missing: 2,
    }
}

fn assert_same_requests(server: &MockHttp, count: usize) {
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), count);
    assert!(
        requests
            .windows(2)
            .all(|pair| pair[0].raw_body == pair[1].raw_body)
    );
}

#[tokio::test]
async fn transient_failures_retry_once_with_identical_json() {
    for (label, first) in [
        ("503", json_reply(503, "{}")),
        ("429", json_reply(429, "{}")),
        ("disconnect", Reply::Hangup),
        ("truncated 200", truncated_reply(200)),
        ("truncated 400", truncated_reply(400)),
        ("invalid 200 JSON", json_reply(200, "{")),
    ] {
        let server = MockHttp::start(vec![first, success()]);
        let client = client(&server, reqwest::Client::builder().no_proxy())
            .with_sleeper(|_| std::future::ready(()));
        let (_sender, mut cancel) = watch::channel(false);
        let response = client
            .chat(&request(), &mut cancel)
            .await
            .unwrap_or_else(|error| panic!("{label}: {error}"));
        assert_eq!(response.choices[0].message.content, "one", "{label}");
        assert_same_requests(&server, 2);
    }
}

#[tokio::test]
async fn retry_exhaustion_reports_last_error_and_attempt_count() {
    for (first, second, final_status) in [
        (503, 502, Some(502)),
        (429, 429, Some(429)),
        (200, 200, None),
    ] {
        let server = MockHttp::start(vec![json_reply(first, "{"), json_reply(second, "broken")]);
        let client = client(&server, reqwest::Client::builder().no_proxy())
            .with_sleeper(|_| std::future::ready(()));
        let (_sender, mut cancel) = watch::channel(false);
        let error = client.chat(&request(), &mut cancel).await.unwrap_err();
        let description = error.to_string();
        let Error::Exhausted {
            attempts: 2,
            source,
        } = error
        else {
            panic!("{error:?}")
        };
        match (final_status, &*source) {
            (Some(status), Error::Api(api)) => {
                assert_eq!(api.status, status);
                assert_eq!(api.message, format!("status {status}: broken"));
            }
            (None, Error::Decode(_)) => {}
            _ => panic!("{source:?}"),
        }
        assert_eq!(description, format!("{source} (after 2 attempts)"));
        assert_same_requests(&server, 2);
    }
}

#[tokio::test]
async fn permanent_errors_keep_status_without_attempt_annotation() {
    for status in [400, 401, 403, 404] {
        let server = MockHttp::start(vec![json_reply(status, "{}")]);
        let client = client(&server, reqwest::Client::builder().no_proxy())
            .with_sleeper(|_| async { panic!("permanent error entered backoff") });
        let (_sender, mut cancel) = watch::channel(false);
        let error = client.chat(&request(), &mut cancel).await.unwrap_err();
        assert!(matches!(error, Error::Api(ref api) if api.status == status));
        assert!(!error.to_string().contains("attempts"));
        assert_same_requests(&server, 1);
    }
    let server = MockHttp::start(vec![json_reply(503, "{}"), json_reply(401, "{}")]);
    let client = client(&server, reqwest::Client::builder().no_proxy())
        .with_sleeper(|_| std::future::ready(()));
    let (_sender, mut cancel) = watch::channel(false);
    let error = client.chat(&request(), &mut cancel).await.unwrap_err();
    assert!(matches!(error, Error::Api(ref api) if api.status == 401));
    assert!(!error.to_string().contains("attempts"));
    assert_same_requests(&server, 2);
}

#[tokio::test]
async fn builder_cannot_add_transport_retries_to_attempt_budget() {
    let server = MockHttp::start((0..4).map(|_| json_reply(503, "{}")).collect());
    let automatic_retry = reqwest::retry::for_host("127.0.0.1")
        .no_budget()
        .max_retries_per_request(1)
        .classify_fn(|attempt| {
            if attempt.status() == Some(reqwest::StatusCode::SERVICE_UNAVAILABLE) {
                attempt.retryable()
            } else {
                attempt.success()
            }
        });
    let client = client(
        &server,
        reqwest::Client::builder().no_proxy().retry(automatic_retry),
    )
    .with_sleeper(|_| std::future::ready(()));
    let (_sender, mut cancel) = watch::channel(false);
    let error = client.chat(&request(), &mut cancel).await.unwrap_err();
    assert!(
        matches!(error, Error::Exhausted { attempts: 2, source } if matches!(&*source, Error::Api(api) if api.status == 503))
    );
    assert_same_requests(&server, 2);
}

fn read_one_request(listener: std::net::TcpListener) -> std::net::TcpStream {
    use std::io::{BufRead, BufReader, Read};
    let (stream, _) = listener.accept().unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut reader = BufReader::new(stream);
    let mut length = None;
    loop {
        let mut line = String::new();
        assert_ne!(reader.read_line(&mut line).unwrap(), 0);
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = Some(value.trim().parse::<usize>().unwrap());
        }
    }
    let mut body = vec![0; length.unwrap()];
    reader.read_exact(&mut body).unwrap();
    reader.into_inner()
}

fn local_tcp_client(address: std::net::SocketAddr, builder: reqwest::ClientBuilder) -> Client {
    Client::with_options(
        "fixture-key".into(),
        Url::parse(&format!("http://{address}")).unwrap(),
        Duration::from_secs(5),
        builder,
    )
    .unwrap()
}

#[tokio::test]
async fn builder_cannot_enable_redirects() {
    use std::io::Write;
    let target = MockHttp::start(vec![success()]);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let client = local_tcp_client(
        listener.local_addr().unwrap(),
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::limited(4)),
    );
    let location = format!("http://{}/unexpected", target.addr);
    let response = std::thread::spawn(move || {
        let mut socket = read_one_request(listener);
        write!(socket, "HTTP/1.1 307 Temporary Redirect\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let (_sender, mut cancel) = watch::channel(false);
    let error = client.chat(&request(), &mut cancel).await.unwrap_err();
    response.join().unwrap();
    assert!(matches!(error, Error::Api(api) if api.status == 307));
    assert_same_requests(&target, 0);
}

#[tokio::test]
async fn cancels_while_waiting_for_response_headers() {
    use tokio::sync::oneshot;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let client = local_tcp_client(
        listener.local_addr().unwrap(),
        reqwest::Client::builder().no_proxy(),
    )
    .with_sleeper(|_| async { panic!("cancelled request entered backoff") });
    let (accepted, received) = oneshot::channel();
    let (release, hold) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let _connection = read_one_request(listener);
        accepted.send(()).unwrap();
        hold.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let (sender, mut cancel) = watch::channel(false);
    let input = request();
    let operation = client.chat(&input, &mut cancel);
    tokio::pin!(operation);
    tokio::select! {
        result = &mut operation => panic!("request finished before cancellation: {result:?}"),
        signal = tokio::time::timeout(Duration::from_secs(2), received) => signal.unwrap().unwrap(),
    }
    sender.send_replace(true);
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), operation)
            .await
            .unwrap(),
        Err(Error::Cancelled)
    ));
    release.send(()).unwrap();
    server.join().unwrap();
}

fn timed_client(
    server: &MockHttp,
) -> (
    Client,
    watch::Sender<Duration>,
    tokio::sync::mpsc::UnboundedReceiver<Duration>,
) {
    let (clock, elapsed) = watch::channel(Duration::ZERO);
    let (signals, entered) = tokio::sync::mpsc::unbounded_channel();
    let client = client(server, reqwest::Client::builder().no_proxy()).with_sleeper(move |delay| {
        let mut elapsed = elapsed.clone();
        let signals = signals.clone();
        async move {
            signals.send(delay).unwrap();
            elapsed.wait_for(|elapsed| *elapsed >= delay).await.unwrap();
        }
    });
    (client, clock, entered)
}

async fn wait_for_backoff<F: std::future::Future>(
    mut operation: std::pin::Pin<&mut F>,
    entered: &mut tokio::sync::mpsc::UnboundedReceiver<Duration>,
) where
    F::Output: std::fmt::Debug,
{
    tokio::select! {
        biased;
        signal = tokio::time::timeout(Duration::from_secs(2), entered.recv()) => {
            assert_eq!(signal.unwrap().unwrap(), Duration::from_millis(500));
        }
        result = &mut operation => panic!("finished before entering backoff: {result:?}"),
    }
}

#[tokio::test]
async fn second_attempt_waits_for_five_hundred_virtual_milliseconds() {
    let server = MockHttp::start(vec![json_reply(503, "{}"), success()]);
    let (client, clock, mut entered) = timed_client(&server);
    let (_sender, mut cancel) = watch::channel(false);
    let input = request();
    let operation = client.chat(&input, &mut cancel);
    tokio::pin!(operation);
    wait_for_backoff(operation.as_mut(), &mut entered).await;
    clock.send_replace(Duration::from_millis(499));
    tokio::select! {
        biased;
        result = &mut operation => panic!("finished before the backoff deadline: {result:?}"),
        _ = std::future::ready(()) => {}
    }
    assert_same_requests(&server, 1);

    clock.send_replace(Duration::from_millis(500));
    let result = tokio::time::timeout(Duration::from_secs(2), operation)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.choices[0].message.content, "one");
    assert_same_requests(&server, 2);
}

#[tokio::test]
async fn cancellation_stops_backoff_at_zero_virtual_time() {
    let server = MockHttp::start(vec![json_reply(503, "{}"), success()]);
    let (client, clock, mut entered) = timed_client(&server);
    let (sender, mut cancel) = watch::channel(false);
    let input = request();
    let operation = client.chat(&input, &mut cancel);
    tokio::pin!(operation);
    wait_for_backoff(operation.as_mut(), &mut entered).await;
    sender.send_replace(true);
    tokio::select! {
        biased;
        result = &mut operation => {
            let error = result.unwrap_err();
            assert!(matches!(error, Error::Cancelled));
            assert!(!error.to_string().contains("attempts"));
        }
        _ = std::future::ready(()) => panic!("backoff did not cancel at 0ms"),
    }
    clock.send_replace(Duration::from_secs(1));
    assert_same_requests(&server, 1);
}
