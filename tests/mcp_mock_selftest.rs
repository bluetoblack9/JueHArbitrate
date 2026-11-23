mod support;
use std::io::{BufRead, BufReader, Read, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

struct OwnedChild {
    child: Child,
    reaped: bool,
}
impl OwnedChild {
    fn terminate(&mut self) -> ExitStatus {
        let _ = self.child.kill();
        let status = self.child.wait().expect("必须回收本测试启动的辅助进程");
        self.reaped = true;
        status
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

struct PeerIo {
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    trace: Arc<Mutex<Vec<String>>>,
}
impl PeerIo {
    fn send(&mut self, request: serde_json::Value) -> Result<(), String> {
        self.trace.lock().unwrap().push(request.to_string());
        writeln!(self.input, "{request}").map_err(|e| format!("write request: {e}"))
    }
    fn recv(&mut self) -> Result<serde_json::Value, String> {
        let mut line = String::new();
        let n = self
            .output
            .read_line(&mut line)
            .map_err(|e| format!("read response: {e}"))?;
        if n == 0 {
            return Err("辅助进程提前关闭 stdout".into());
        }
        serde_json::from_str(&line).map_err(|e| format!("invalid response {line:?}: {e}"))
    }
}

fn run_peer<F>(script: &str, budget: Duration, session: F) -> Result<(), String>
where
    F: FnOnce(&mut PeerIo) -> Result<(), String> + Send + 'static,
{
    let root = support::TempRoot::new("mcp");
    let script_path = root.path.join("script.json");
    std::fs::write(&script_path, script).unwrap();
    let deadline = Instant::now() + budget;
    let child = Command::new(env!("CARGO_BIN_EXE_mcp-mock"))
        .arg(&script_path)
        .current_dir(&root.path)
        .env("SEEK_HOME", root.user_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut child = OwnedChild {
        child,
        reaped: false,
    };
    let input = child.child.stdin.take().unwrap();
    let output = BufReader::new(child.child.stdout.take().unwrap());
    let mut stderr = child.child.stderr.take().unwrap();
    let stderr_thread = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });
    let trace = Arc::new(Mutex::new(Vec::new()));
    let worker_trace = trace.clone();
    let (tx, rx) = mpsc::channel();
    let io_thread = thread::spawn(move || {
        let mut io = PeerIo {
            input,
            output,
            trace: worker_trace,
        };
        let result = catch_unwind(AssertUnwindSafe(|| session(&mut io)))
            .unwrap_or_else(|_| Err("session assertions panicked".into()));
        drop(io); // 正常结束时关闭 stdin，让辅助进程看到 EOF。
        let _ = tx.send(result);
    });
    let mut result = match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            Err("deadline exceeded waiting for session I/O".into())
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => Err("session worker disconnected".into()),
    };
    let status = loop {
        match child.child.try_wait() {
            Ok(Some(status)) => {
                child.reaped = true;
                break status;
            }
            Ok(None) if result.is_err() || Instant::now() >= deadline => {
                if result.is_ok() {
                    result = Err("deadline exceeded waiting for exit".into());
                }
                break child.terminate();
            }
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(e) => {
                result = Err(format!("try_wait: {e}"));
                break child.terminate();
            }
        }
    };
    // 该辅助进程不启动后代；退出/终止并 wait 后，管道 EOF 会解除所有 I/O 等待。
    if io_thread.join().is_err() {
        result = Err("I/O worker unexpectedly panicked".into());
    }
    let stderr = stderr_thread
        .join()
        .unwrap_or_else(|_| b"stderr reader panicked".to_vec());
    if !status.success() && result.is_ok() {
        result = Err(format!("helper exited: {status}"));
    }
    result.map_err(|reason| {
        format!(
            "{reason}; reaped={}; requests={:?}; script={script}; stderr={}",
            child.reaped,
            trace.lock().unwrap(),
            String::from_utf8_lossy(&stderr)
        )
    })
}

#[test]
fn scripted_initialize_and_tools_list() {
    run_peer(
        &support::load_fixture("mcp-script.json"),
        Duration::from_secs(5),
        |io| {
            io.send(
                serde_json::json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{}}),
            )?;
            let v = io.recv()?;
            assert_eq!(v["id"], 1);
            assert_eq!(v["result"]["serverInfo"]["name"], "mock-mcp");
            io.send(serde_json::json!({"jsonrpc":"2.0", "method":"notifications/initialized"}))?;
            io.send(serde_json::json!({"jsonrpc":"2.0", "id":2, "method":"tools/list"}))?;
            let v = io.recv()?;
            assert_eq!(v["id"], 2, "notification 不得产生回包");
            assert_eq!(v["result"]["tools"].as_array().unwrap().len(), 2);
            Ok(())
        },
    )
    .unwrap();
}

#[test]
fn unmatched_request_returns_method_not_found() {
    run_peer("[]", Duration::from_secs(5), |io| {
        io.send(serde_json::json!({"jsonrpc":"2.0", "id":7, "method":"missing"}))?;
        let v = io.recv()?;
        assert_eq!(v["id"], 7);
        assert_eq!(v["error"]["code"], -32601);
        Ok(())
    })
    .unwrap();
}

#[test]
fn stalled_peer_is_killed_and_reaped() {
    let script = r#"[{"match_method":"initialize","stall":true}]"#;
    let started = Instant::now();
    let err = run_peer(script, Duration::from_millis(200), |io| {
        io.send(serde_json::json!({"jsonrpc":"2.0", "id":1, "method":"initialize"}))?;
        io.recv()?;
        Ok(())
    })
    .unwrap_err();
    assert!(err.contains("deadline exceeded"), "{err}");
    assert!(err.contains("reaped=true"), "{err}");
    assert!(err.contains("requests=") && err.contains("script=") && err.contains("stderr="));
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "截止时间后未及时回收"
    );
}
