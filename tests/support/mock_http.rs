// ponytail: std-only HTTP/1.1 mock，无 TLS/HTTP2；provider mock 足够，需要时换 hyper。
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum Reply {
    Json { status: u16, body: String },
    Sse { chunks: Vec<String>, delay_ms: u64 },
    Hangup,
}

#[derive(Clone, Debug)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub body: String,
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
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(());
    }
    requests.lock().unwrap().push(Recorded {
        method: parts[0].into(),
        path: parts[1].into(),
        body: String::from_utf8_lossy(&body).into_owned(),
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
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
            )?;
            stream.flush()?;
            for c in chunks {
                write!(stream, "{:x}\r\n{c}\r\n", c.len())?;
                stream.flush()?;
                if delay_ms > 0 {
                    thread::sleep(Duration::from_millis(delay_ms));
                }
            }
            write!(stream, "0\r\n\r\n")?;
            stream.flush()
        }
    }
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
