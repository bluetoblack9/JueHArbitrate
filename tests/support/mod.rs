#![allow(dead_code)]
pub mod mock_http;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct TempRoot {
    pub path: PathBuf,
}

impl TempRoot {
    pub fn new(tag: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("arbitrate-test-{tag}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        TempRoot { path }
    }
    pub fn user_root(&self) -> PathBuf {
        let p = self.path.join("user");
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    pub fn project(&self, name: &str) -> PathBuf {
        let p = self.path.join(name);
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    pub fn seek_home_env(&self) -> (String, String) {
        (
            "SEEK_HOME".into(),
            self.user_root().to_string_lossy().into_owned(),
        )
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

pub struct FixedClock {
    pub epoch_secs: i64,
}
impl FixedClock {
    pub fn now(&self) -> i64 {
        self.epoch_secs
    }
    pub fn advance(&mut self, secs: i64) {
        self.epoch_secs += secs;
    }
}

pub fn fixed_id(prefix: &str, n: u32) -> String {
    format!("{prefix}-{n:06}")
}

pub fn load_fixture(rel: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel);
    std::fs::read_to_string(p).unwrap()
}

#[derive(Clone, Debug)]
pub struct StubOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Debug)]
pub struct StubCall {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

pub trait CommandRunner {
    fn run(&mut self, program: &str, args: &[String], env: &[(String, String)]) -> StubOutput;
}

pub struct RecordingRunner {
    pub calls: Vec<StubCall>,
    canned: StubOutput,
}
impl RecordingRunner {
    pub fn new(canned: StubOutput) -> Self {
        RecordingRunner {
            calls: Vec::new(),
            canned,
        }
    }
}
impl CommandRunner for RecordingRunner {
    fn run(&mut self, program: &str, args: &[String], env: &[(String, String)]) -> StubOutput {
        self.calls.push(StubCall {
            program: program.into(),
            args: args.to_vec(),
            env: env.to_vec(),
        });
        self.canned.clone()
    }
}
