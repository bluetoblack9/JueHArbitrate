// ponytail: std-only HTTP/1.1 mock，无 TLS/HTTP2；provider mock 足够，需要时换 hyper。
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum Reply {
    Json {
        status: u16,
        body: String,
    },
    Sse {
        chunks: Vec<String>,
        delay_ms: u64,
    },
    Bytes {
        content_type: String,
        body: Vec<u8>,
    },
    Truncated {
        status: u16,
        prefix: Vec<u8>,
        missing: usize,
    },
    PausedHeaders {
        release: Arc<(Mutex<bool>, Condvar)>,
    },
    PausedSse {
        first: String,
        release: Arc<(Mutex<bool>, Condvar)>,
    },
    Hangup,
}

#[derive(Clone, Debug)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub body: String,
    pub headers: Vec<(String, Vec<u8>)>,
    pub raw_body: Vec<u8>,
}

pub struct MockHttp {
    pub addr: SocketAddr,
    pub requests: Arc<Mutex<Vec<Recorded>>>,
    stop: Arc<AtomicBool>,
}

impl MockHttp {
    pub fn start(script: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let queue = Arc::new(Mutex::new(VecDeque::from(script)));
        let (r2, s2, q2) = (requests.clone(), stop.clone(), queue.clone());
        thread::spawn(move || {
            while !s2.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let (r3, q3) = (r2.clone(), q2.clone());
                        thread::spawn(move || {
                            let _ = handle(stream, r3, q3);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
        });
        MockHttp {
            addr,
            requests,
            stop,
        }
    }
}

impl Drop for MockHttp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn handle(
    mut stream: TcpStream,
    requests: Arc<Mutex<Vec<Recorded>>>,
    queue: Arc<Mutex<VecDeque<Reply>>>,
) -> std::io::Result<()> {
    // Accepted sockets can inherit the listener's nonblocking mode (e.g. on macOS).
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut headers = Vec::new();
    let mut content_length = 0usize;
    let mut chunked = false;
    loop {
        let line = read_crlf_line(&mut reader)?;
        if line.is_empty() {
            break;
        }
        let colon = line
            .iter()
            .position(|&byte| byte == b':')
            .ok_or_else(invalid_request)?;
        let name = std::str::from_utf8(&line[..colon])
            .map_err(|_| invalid_request())?
            .to_ascii_lowercase();
        let value = line[colon + 1..].trim_ascii();
        if name == "content-length" {
            content_length = std::str::from_utf8(value)
                .ok()
                .and_then(|value| value.parse().ok())
                .ok_or_else(invalid_request)?;
        }
        if name == "transfer-encoding" && value.eq_ignore_ascii_case(b"chunked") {
            chunked = true;
        }
        headers.push((name, value.to_vec()));
    }
    let body = if chunked {
        read_chunked(&mut reader)?
    } else {
        let mut body = Vec::new();
        read_body_bytes(&mut reader, content_length, &mut body)?;
        body
    };
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(());
    }
    requests.lock().unwrap().push(Recorded {
        method: parts[0].into(),
        path: parts[1].into(),
        body: String::from_utf8_lossy(&body).into_owned(),
        headers,
        raw_body: body,
    });
    let reply = queue.lock().unwrap().pop_front().unwrap_or(Reply::Json {
        status: 500,
        body: "{}".into(),
    });
    match reply {
        Reply::Hangup => Ok(()), // 直接断连
        Reply::Json { status, body } => {
            write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )?;
            stream.flush()
        }
        Reply::Sse { chunks, delay_ms } => {
            write_sse_headers(&mut stream)?;
            for c in chunks {
                write_sse_chunk(&mut stream, &c)?;
                if delay_ms > 0 {
                    thread::sleep(Duration::from_millis(delay_ms));
                }
            }
            write!(stream, "0\r\n\r\n")?;
            stream.flush()
        }
        Reply::Bytes { content_type, body } => {
            write_bytes(&mut stream, 200, &content_type, body.len(), &body)
        }
        Reply::Truncated {
            status,
            prefix,
            missing,
        } => {
            let declared = prefix
                .len()
                .checked_add(missing)
                .ok_or_else(invalid_request)?;
            write_bytes(&mut stream, status, "text/event-stream", declared, &prefix)
        }
        Reply::PausedHeaders { release } => {
            wait_for_release(&release);
            Ok(())
        }
        Reply::PausedSse { first, release } => {
            write_sse_headers(&mut stream)?;
            write_sse_chunk(&mut stream, &first)?;
            wait_for_release(&release);
            stream.write_all(b"0\r\n\r\n")?;
            stream.flush()
        }
    }
}

fn wait_for_release((released, signal): &(Mutex<bool>, Condvar)) {
    let released = released.lock().unwrap();
    let _wait = signal
        .wait_timeout_while(released, Duration::from_secs(5), |released| !*released)
        .unwrap();
}

fn invalid_request() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid mock HTTP framing")
}

fn read_crlf_line(reader: &mut impl BufRead) -> std::io::Result<Vec<u8>> {
    let mut line = Vec::new();
    reader.read_until(b'\n', &mut line)?;
    if !line.ends_with(b"\r\n") {
        return Err(invalid_request());
    }
    line.truncate(line.len() - 2);
    Ok(line)
}

fn read_body_bytes(
    reader: &mut impl Read,
    length: usize,
    body: &mut Vec<u8>,
) -> std::io::Result<()> {
    if reader.take(length as u64).read_to_end(body)? != length {
        return Err(std::io::ErrorKind::UnexpectedEof.into());
    }
    Ok(())
}

fn read_chunked(reader: &mut impl BufRead) -> std::io::Result<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let line = read_crlf_line(reader)?;
        let size = line.split(|&byte| byte == b';').next().unwrap_or_default();
        let size = std::str::from_utf8(size.trim_ascii())
            .ok()
            .and_then(|value| usize::from_str_radix(value, 16).ok())
            .ok_or_else(invalid_request)?;
        if size == 0 {
            while !read_crlf_line(reader)?.is_empty() {}
            return Ok(body);
        }
        read_body_bytes(reader, size, &mut body)?;
        let mut separator = [0; 2];
        reader.read_exact(&mut separator)?;
        if separator != *b"\r\n" {
            return Err(invalid_request());
        }
    }
}

fn write_bytes(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    declared: usize,
    body: &[u8],
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: {content_type}\r\nContent-Length: {declared}\r\nConnection: close\r\n\r\n"
    )?;
    stream.write_all(body)?;
    stream.flush()
}

fn write_sse_headers(stream: &mut TcpStream) -> std::io::Result<()> {
    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n")?;
    stream.flush()
}

fn write_sse_chunk(stream: &mut TcpStream, chunk: &str) -> std::io::Result<()> {
    if !chunk.is_empty() {
        write!(stream, "{:x}\r\n{chunk}\r\n", chunk.len())?;
        stream.flush()?;
    }
    Ok(())
}

pub fn http_request(
    addr: SocketAddr,
    method: &str,
    path: &str,
    body: &str,
) -> std::io::Result<(u16, String)> {
    let mut s = TcpStream::connect(addr)?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    write!(
        s,
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    s.flush()?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw)?;
    let text = String::from_utf8_lossy(&raw).into_owned();
    let status: u16 = text
        .split_whitespace()
        .nth(1)
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    if status == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "no status line",
        ));
    }
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();
    Ok((status, body))
}

pub fn dechunk(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    loop {
        let Some((line, tail)) = rest.split_once("\r\n") else {
            break;
        };
        let n = usize::from_str_radix(line.trim(), 16).unwrap_or(0);
        if n == 0 {
            break;
        }
        out.push_str(&tail[..n]);
        rest = &tail[n + 2..];
    }
    out
}

pub fn sse_events(data: &str) -> Vec<String> {
    data.split("\n\n")
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .collect()
}
