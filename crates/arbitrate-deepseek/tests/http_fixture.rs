#[allow(dead_code)]
#[path = "../../../tests/support/mock_http.rs"]
mod mock_http;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use mock_http::{MockHttp, Reply};
use reqwest::multipart::{Form, Part};

#[tokio::test]
async fn records_multipart_order_headers_and_binary_payload() {
    let server = MockHttp::start(vec![Reply::Json {
        status: 200,
        body: "{}".into(),
    }]);
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let form = Form::new()
        .text("purpose", "user_data")
        .part("file", Part::bytes(vec![0, 255, 1]).file_name("a.bin"));
    let response = client
        .post(format!("http://{}/files", server.addr))
        .header("Authorization", "Bearer fake-test-key")
        .header("X-Test", "one")
        .header("X-Test", "two")
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let requests = server.requests.lock().unwrap();
    let request = &requests[0];
    let headers = |name| {
        request
            .headers
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.as_slice())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        headers("authorization"),
        [b"Bearer fake-test-key".as_slice()]
    );
    assert_eq!(headers("x-test"), [b"one", b"two"]);
    let content_types = headers("content-type");
    let boundary = std::str::from_utf8(content_types[0])
        .unwrap()
        .strip_prefix("multipart/form-data; boundary=")
        .unwrap();
    let delimiter = format!("--{boundary}");
    let mut rest = request.raw_body.strip_prefix(delimiter.as_bytes()).unwrap();
    let next = format!("\r\n{delimiter}");
    for (disposition, payload) in [
        ("form-data; name=\"purpose\"", b"user_data".as_slice()),
        ("form-data; name=\"file\"; filename=\"a.bin\"", &[0, 255, 1]),
    ] {
        rest = rest.strip_prefix(b"\r\n").unwrap();
        let header_end = rest
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
            .unwrap();
        let head = std::str::from_utf8(&rest[..header_end]).unwrap();
        assert!(
            head.split("\r\n")
                .any(|line| line == format!("Content-Disposition: {disposition}"))
        );
        rest = &rest[header_end + 4..];
        let payload_end = rest
            .windows(next.len())
            .position(|part| part == next.as_bytes())
            .unwrap();
        assert_eq!(&rest[..payload_end], payload);
        rest = &rest[payload_end + next.len()..];
    }
    assert_eq!(rest, b"--\r\n");
}

#[test]
fn records_chunked_request_bytes_after_consuming_trailers() {
    let server = MockHttp::start(vec![Reply::Json {
        status: 200,
        body: "{}".into(),
    }]);
    let mut stream = TcpStream::connect(server.addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(b"POST /bytes HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n\x00\xff\r\n1\r\n\x01\r\n0\r\nX-Trailer: end\r\n\r\n").unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).unwrap();
    assert!(reply.starts_with("HTTP/1.1 200 "));
    assert_eq!(server.requests.lock().unwrap()[0].raw_body, [0, 255, 1]);
}

#[tokio::test]
async fn transport_preserves_the_base_path_and_injected_timeout() {
    use arbitrate_deepseek::transport::{DEFAULT_REQUEST_TIMEOUT, Transport};
    use reqwest::{Client, Method, Url};

    let server = MockHttp::start(vec![Reply::Json {
        status: 200,
        body: "{}".into(),
    }]);
    let base = Url::parse(&format!("http://{}/proxy/v1", server.addr)).unwrap();
    let client = Client::builder().no_proxy().build().unwrap();
    let transport = Transport::new(client.clone(), base.clone(), DEFAULT_REQUEST_TIMEOUT);
    let request = transport
        .request(Method::POST, "/chat/completions")
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(request.timeout(), Some(&Duration::from_secs(300)));
    assert_eq!(client.execute(request).await.unwrap().status(), 200);
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests[0].path, "/proxy/v1/chat/completions");
    assert!(
        !requests[0]
            .headers
            .iter()
            .any(|(key, _)| key == "x-opencode-session")
    );

    let timeout = Duration::from_millis(30);
    let transport = Transport::new(client, base, timeout);
    let request = transport
        .request(Method::GET, "test")
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(request.timeout(), Some(&timeout));
    for endpoint in [
        "https://example.invalid/x",
        "//example.invalid/x",
        " //example.invalid/proxy/v1/x",
        "/../x",
        "x?y=z",
        "x#y",
    ] {
        assert!(
            transport.request(Method::GET, endpoint).is_err(),
            "{endpoint}"
        );
    }
}

async fn sleep_until(mut clock: tokio::sync::watch::Receiver<Duration>, delay: Duration) {
    clock.wait_for(|elapsed| *elapsed >= delay).await.unwrap();
}

fn retry_server() -> (MockHttp, reqwest::Client, String) {
    let server = MockHttp::start(vec![
        Reply::Json {
            status: 503,
            body: "{}".into(),
        },
        Reply::Json {
            status: 200,
            body: "{}".into(),
        },
    ]);
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let endpoint = format!("http://{}/retry", server.addr);
    (server, client, endpoint)
}

async fn poll_once<F: std::future::Future>(
    mut future: std::pin::Pin<&mut F>,
) -> std::task::Poll<F::Output> {
    tokio::select! {
        biased;
        result = &mut future => std::task::Poll::Ready(result),
        _ = std::future::ready(()) => std::task::Poll::Pending,
    }
}

#[tokio::test]
async fn does_not_send_again_before_the_production_retry_delay() {
    use arbitrate_deepseek::transport::wait_retry;
    use std::task::Poll;
    use tokio::sync::watch;

    let (server, client, endpoint) = retry_server();
    assert_eq!(client.post(&endpoint).send().await.unwrap().status(), 503);
    let (_cancel, mut cancelled) = watch::channel(false);
    let (clock, elapsed) = watch::channel(Duration::ZERO);
    let wait = wait_retry(&mut cancelled, |delay| sleep_until(elapsed, delay));
    tokio::pin!(wait);
    assert!(poll_once(wait.as_mut()).await.is_pending());
    clock.send_replace(Duration::from_millis(499));
    assert!(poll_once(wait.as_mut()).await.is_pending());
    assert_eq!(server.requests.lock().unwrap().len(), 1);

    clock.send_replace(Duration::from_millis(500));
    assert_eq!(poll_once(wait.as_mut()).await, Poll::Ready(Ok(())));
    assert_eq!(client.post(&endpoint).send().await.unwrap().status(), 200);
    assert_eq!(server.requests.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn cancelling_retry_does_not_wait_for_the_clock_or_send_again() {
    use arbitrate_deepseek::transport::{Cancelled, wait_retry};
    use std::task::Poll;
    use tokio::sync::watch;

    let (server, client, endpoint) = retry_server();
    assert_eq!(client.post(&endpoint).send().await.unwrap().status(), 503);
    let (cancel, mut cancelled) = watch::channel(false);
    let (clock, elapsed) = watch::channel(Duration::ZERO);
    let attempt = async {
        wait_retry(&mut cancelled, |delay| sleep_until(elapsed, delay)).await?;
        Ok(client.post(&endpoint).send().await.unwrap().status())
    };
    tokio::pin!(attempt);
    assert!(poll_once(attempt.as_mut()).await.is_pending());
    cancel.send_replace(true);
    assert_eq!(
        poll_once(attempt.as_mut()).await,
        Poll::Ready(Err(Cancelled))
    );
    clock.send_replace(Duration::from_secs(1));
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn cancelling_a_pending_sse_read_closes_the_event_channel() {
    use arbitrate_deepseek::transport::{Cancelled, until_cancelled};
    use std::sync::{Arc, Condvar, Mutex};
    use tokio::sync::{mpsc, oneshot, watch};
    use tokio::time::timeout;

    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let server = MockHttp::start(vec![Reply::PausedSse {
        first: "data: first\n\n".into(),
        release: release.clone(),
    }]);
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let mut response = client
        .get(format!("http://{}/events", server.addr))
        .send()
        .await
        .unwrap();
    let (cancel, mut cancelled) = watch::channel(false);
    let (events, mut received) = mpsc::channel(1);
    let (reading, read_pending) = oneshot::channel();
    let pump = tokio::spawn(async move {
        let first = response.chunk().await.unwrap().unwrap();
        events.send(first).await.unwrap();
        let next = until_cancelled(&mut cancelled, response.chunk());
        tokio::pin!(next);
        assert!(poll_once(next.as_mut()).await.is_pending());
        reading.send(()).unwrap();
        next.await.map(|_| ())
    });
    timeout(Duration::from_secs(2), read_pending)
        .await
        .unwrap()
        .unwrap();
    cancel.send_replace(true);
    assert_eq!(
        timeout(Duration::from_secs(2), pump)
            .await
            .unwrap()
            .unwrap(),
        Err(Cancelled)
    );
    assert_eq!(&received.recv().await.unwrap()[..], b"data: first\n\n");
    assert!(received.recv().await.is_none());
    assert!(!*release.0.lock().unwrap());
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}
