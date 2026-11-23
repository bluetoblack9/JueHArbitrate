mod support;
use support::*;

#[test]
fn temp_roots_isolated_and_cleaned() {
    let path_a;
    {
        let a = TempRoot::new("iso-a");
        let b = TempRoot::new("iso-b");
        assert_ne!(a.user_root(), b.user_root());
        assert!(a.project("p1").is_dir());
        path_a = a.path.clone();
        let (k, v) = a.seek_home_env();
        assert_eq!(k, "SEEK_HOME");
        assert_eq!(v, a.user_root().to_string_lossy());
    }
    assert!(!path_a.exists(), "Drop 必须清理临时根");
}

#[test]
fn fixed_clock_deterministic() {
    let mut c = FixedClock {
        epoch_secs: 1_700_000_000,
    };
    assert_eq!(c.now(), 1_700_000_000);
    c.advance(3600);
    assert_eq!(c.now(), 1_700_003_600);
    assert_eq!(fixed_id("sess", 3), "sess-000003");
}

#[test]
fn recording_runner_captures_env_without_real_exec() {
    let mut r = RecordingRunner::new(StubOutput {
        status: 0,
        stdout: "ok".into(),
        stderr: String::new(),
    });
    let out = r.run(
        "git",
        &["status".into()],
        &[("SEEK_HOME".into(), "/tmp/fake".into())],
    );
    assert_eq!(out.stdout, "ok");
    assert_eq!(r.calls.len(), 1);
    assert_eq!(r.calls[0].program, "git");
    assert_eq!(r.calls[0].env[0], ("SEEK_HOME".into(), "/tmp/fake".into()));
}
