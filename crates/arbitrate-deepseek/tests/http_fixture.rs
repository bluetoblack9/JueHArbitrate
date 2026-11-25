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
