//! core::pathop 契约测试 — 规范化/expand_env/Windows 可注入算法（规格：上游
//! internal/pathop，Rust 自有重写）。Windows 算法经 stub 跨平台验证，绝不触
//! 真实注册表/PATH（约束：测试注入替身）。%VAR% 不展开（上游注释漂移钉死）。

use arbitrate_core::pathop::{
    PathopError, ValueKind, WindowsBackend, clean_posix, clean_windows, ensure_in_path_windows,
    expand_env_with, is_in_user_path_windows, user_path_contains_with,
};

#[test]
fn clean_posix_table() {
    for (input, want) in [
        ("", "."),
        ("/a/b/../c", "/a/c"),
        ("a/../..", ".."),
        ("/..", "/"),
        ("a//b", "a/b"),
        ("./a", "a"),
        ("../a", "../a"),
        ("a/b/..", "a"),
        ("/", "/"),
    ] {
        assert_eq!(clean_posix(input), want, "clean_posix({input:?})");
    }
}

#[test]
fn clean_windows_table() {
    for (input, want) in [
        ("", "."),
        ("C:\\Tools\\", "C:\\Tools"),
        ("C:\\Tools\\..\\Bin", "C:\\Bin"),
        ("C:\\a\\\\b", "C:\\a\\b"),
        ("C:", "C:."),
        ("C:\\", "C:\\"),
        ("Tools\\", "Tools"),
        ("C:/Tools/", "C:\\Tools"),
        ("\\\\host\\share", "\\\\host\\share"),
    ] {
        assert_eq!(clean_windows(input), want, "clean_windows({input:?})");
    }
}

#[test]
fn expand_env_table() {
    let home = |n: &str| {
        if n == "HOME" {
            "/home/u".to_string()
        } else {
            String::new()
        }
    };
    assert_eq!(expand_env_with("$HOME/bin", home), "/home/u/bin");
    assert_eq!(expand_env_with("${HOME}/bin", home), "/home/u/bin");
    assert_eq!(expand_env_with("$MISSING/x", home), "/x"); // 缺变量 → ""
    assert_eq!(expand_env_with("plain", home), "plain");
    assert_eq!(expand_env_with("$", home), "$"); // lone $ 保留
    assert_eq!(expand_env_with("$ ", home), "$ ");
    assert_eq!(expand_env_with("$$", home), ""); // $ 是 special var → mapping("$")=""
    assert_eq!(expand_env_with("${}", home), ""); // bad syntax eat "${}" → ""
    assert_eq!(expand_env_with("${a-b}", home), ""); // 花括号扫到 '}'，name="a-b" → mapping=""
    // %VAR% 字面不展开（上游 os.ExpandEnv 是 $-风格，注释 "expanding %VAR%" 是漂移）
    assert_eq!(
        expand_env_with("%USERPROFILE%\\bin", home),
        "%USERPROFILE%\\bin"
    );
}

#[test]
#[cfg(not(windows))]
fn path_contains_dir_posix() {
    use arbitrate_core::pathop::path_contains_dir;
    assert!(path_contains_dir(
        "/usr/bin:/usr/local/bin",
        "/usr/bin",
        false
    ));
    assert!(path_contains_dir("/usr/bin:", "/usr/bin/", false)); // clean 双方
    assert!(!path_contains_dir("/usr/bin", "/USR/BIN", false)); // posix 大小写敏感
    assert!(path_contains_dir("/usr/bin", "/USR/BIN", true)); // fold
    assert!(path_contains_dir(" : /usr/bin : ", "/usr/bin", false)); // 空段跳过 + trim
    assert!(!path_contains_dir("", "/usr/bin", false));
}

#[test]
fn user_path_contains_expand_fold() {
    let get = |n: &str| {
        if n == "HOME" {
            "C:\\Users\\x".to_string()
        } else {
            String::new()
        }
    };
    // $HOME 展开 + 尾分隔符 clean + 大小写折叠
    assert!(user_path_contains_with(
        "$HOME/bin;C:\\Tools\\",
        "c:\\tools",
        get
    ));
    assert!(user_path_contains_with(
        "$HOME/bin",
        "C:\\Users\\x\\bin",
        get
    ));
    // %VAR% 不展开 → 不匹配真实路径
    assert!(!user_path_contains_with(
        "%USERPROFILE%\\bin",
        "C:\\Users\\x\\bin",
        get
    ));
    assert!(!user_path_contains_with("C:\\Other", "C:\\Tools", get));
}

// ---- Windows 算法 stub 替身（绝不触真实注册表/PATH）----
struct Stub {
    user_path: Option<(String, ValueKind)>,
    read_err: bool,
    write_err: bool,
    written: Vec<(String, ValueKind)>,
    proc_path: String,
    proc_set: Vec<String>,
    env: Vec<(String, String)>,
    broadcasts: usize,
}
impl Stub {
    fn new() -> Self {
        Stub {
            user_path: None,
            read_err: false,
            write_err: false,
            written: Vec::new(),
            proc_path: String::new(),
            proc_set: Vec::new(),
            env: Vec::new(),
            broadcasts: 0,
        }
    }
}
impl WindowsBackend for Stub {
    fn abs(&self, dir: &str) -> Result<String, PathopError> {
        Ok(format!("C:\\{dir}")) // 替身：前缀 C:\，已 clean
    }
    fn read_user_path(&self) -> Result<Option<(String, ValueKind)>, PathopError> {
        if self.read_err {
            Err(PathopError::Backend("read fail".into()))
        } else {
            Ok(self.user_path.clone())
        }
    }
    fn write_user_path(&mut self, value: &str, kind: ValueKind) -> Result<(), PathopError> {
        if self.write_err {
            return Err(PathopError::Backend("write fail".into()));
        }
        self.written.push((value.to_string(), kind));
        Ok(())
    }
    fn process_path(&self) -> String {
        self.proc_path.clone()
    }
    fn set_process_path(&mut self, value: &str) {
        self.proc_set.push(value.to_string());
        self.proc_path = value.to_string();
    }
    fn env(&self, name: &str) -> Option<String> {
        self.env
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    }
    fn broadcast(&mut self) {
        self.broadcasts += 1;
    }
}

#[test]
fn ensure_missing_value_uses_expand_sz() {
    let mut b = Stub::new(); // user_path None → ("", ExpandSz)
    let added = ensure_in_path_windows("Tools", false, &mut b).unwrap();
    assert!(added);
    assert_eq!(
        b.written,
        vec![("C:\\Tools".to_string(), ValueKind::ExpandSz)]
    );
    assert_eq!(b.proc_set, vec!["C:\\Tools".to_string()]); // 进程 PATH 空 → absDir
}

#[test]
fn ensure_trailing_semicolon() {
    let mut b = Stub::new();
    b.user_path = Some(("C:\\A;".to_string(), ValueKind::Sz));
    let added = ensure_in_path_windows("Tools", false, &mut b).unwrap();
    assert!(added);
    assert_eq!(
        b.written,
        vec![("C:\\A;C:\\Tools".to_string(), ValueKind::Sz)]
    ); // 尾 ';' 不重复加
}

#[test]
fn ensure_already_present_no_write() {
    let mut b = Stub::new();
    b.user_path = Some(("C:\\Tools".to_string(), ValueKind::Sz));
    let added = ensure_in_path_windows("Tools", false, &mut b).unwrap();
    assert!(!added);
    assert!(b.written.is_empty()); // 已存在 → 不写注册表
    assert_eq!(b.proc_set, vec!["C:\\Tools".to_string()]); // 仍确保进程 PATH
}

#[test]
fn ensure_broadcast_flag() {
    let mut b = Stub::new();
    ensure_in_path_windows("Tools", true, &mut b).unwrap();
    assert_eq!(b.broadcasts, 1);
    let mut b2 = Stub::new();
    ensure_in_path_windows("Tools", false, &mut b2).unwrap();
    assert_eq!(b2.broadcasts, 0);
}

#[test]
fn ensure_process_path_variants() {
    // 进程 PATH 含其他 → append
    let mut b = Stub::new();
    b.user_path = Some(("C:\\Tools".to_string(), ValueKind::Sz)); // already-present 分支
    b.proc_path = "C:\\Other".to_string();
    ensure_in_path_windows("Tools", false, &mut b).unwrap();
    assert_eq!(b.proc_set, vec!["C:\\Other;C:\\Tools".to_string()]);
    // 进程 PATH 已含 → 不 set
    let mut b2 = Stub::new();
    b2.user_path = Some(("C:\\Tools".to_string(), ValueKind::Sz));
    b2.proc_path = "C:\\Tools".to_string();
    ensure_in_path_windows("Tools", false, &mut b2).unwrap();
    assert!(b2.proc_set.is_empty());
}

#[test]
fn ensure_read_error_propagates() {
    let mut b = Stub::new();
    b.read_err = true;
    let err = ensure_in_path_windows("Tools", false, &mut b).unwrap_err();
    assert_eq!(err, PathopError::Backend("read fail".into()));
    assert!(b.written.is_empty());
}

#[test]
fn ensure_write_error_propagates() {
    let mut b = Stub::new();
    b.write_err = true;
    let err = ensure_in_path_windows("Tools", false, &mut b).unwrap_err();
    assert_eq!(err, PathopError::Backend("write fail".into()));
}

#[test]
fn is_in_user_path_stub() {
    let mut b = Stub::new();
    b.user_path = Some(("C:\\Tools".to_string(), ValueKind::Sz));
    assert!(is_in_user_path_windows("Tools", &b).unwrap());
    b.user_path = Some(("C:\\Other".to_string(), ValueKind::Sz));
    assert!(!is_in_user_path_windows("Tools", &b).unwrap());
    b.user_path = None;
    assert!(!is_in_user_path_windows("Tools", &b).unwrap());
}

#[test]
#[cfg(not(windows))]
fn nonwindows_noops() {
    use arbitrate_core::pathop::{ensure_in_path, ensure_in_path_with_broadcast, is_in_user_path};
    assert_eq!(is_in_user_path("/whatever").unwrap(), false);
    assert_eq!(ensure_in_path("/whatever").unwrap(), false);
    assert_eq!(ensure_in_path_with_broadcast("/whatever").unwrap(), false);
}
