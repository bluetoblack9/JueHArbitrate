use std::process::Command;

#[test]
fn scaffold_binary_prints_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_arbitrate"))
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "arbitrate 0.1.0"
    );
}

#[test]
fn scaffold_rejects_unimplemented_entries() {
    for args in [
        vec!["serve"],
        vec!["-p", "hello"],
        vec!["-install"],
        vec!["-version"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_arbitrate"))
            .args(args)
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains("not yet implemented"));
    }
}
