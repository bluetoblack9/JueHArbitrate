use crate::common::TempRoot;
use std::ffi::OsStr;
use std::process::Command;

pub(crate) fn in_temp_dir(name: &str, run: impl FnOnce()) {
    const TEST_NAME: &str = "ARBITRATE_REFERENCE_FIXTURE_TEST";
    if std::env::var_os(TEST_NAME).as_deref() == Some(OsStr::new(name)) {
        run();
        return;
    }

    // Exercise real relative-path stat calls without changing the parent test
    // process's working directory or placing absolute paths in prompt text.
    let root = TempRoot::new();
    let cwd = root.0.join("work");
    std::fs::create_dir(&cwd).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env(TEST_NAME, name)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

macro_rules! reference_test {
    ($name:ident, $body:block) => {
        #[test]
        fn $name() {
            $crate::reference_fixture::in_temp_dir(stringify!($name), || $body);
        }
    };
}

pub(crate) use reference_test;
