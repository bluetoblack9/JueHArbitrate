//! core::pathop — 路径规范化 + Windows 用户 PATH 底层辅助（平台 cfg 隔离）。
//! 规格：上游 internal/pathop（只读参考），Rust std-only 重写。
//! Go build tags → Rust cfg。Windows 算法纯函数化 + 可注入替身（跨平台 stub 测试，
//! 不触真实注册表/PATH）；真实注册表 FFI + WM_SETTINGCHANGE 广播本计划 M0 建（std-only
//! extern，零依赖禁 winreg/windows-sys crate；Windows target 编译检查 + Windows CI 首验）。
//! 非 Windows 公开 API 逐字 no-op（shell rc 编辑超范围）。M3 仅剩 Windows 交互 PATH 提示。

use std::{ffi::OsString, fmt};

/// PATH operation failures, including partial success after a persistent write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathopError {
    Backend(String),
    ProcessPathUpdate {
        user_path_added: bool,
        reason: String,
    },
}
impl fmt::Display for PathopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathopError::Backend(s) => write!(f, "pathop: {s}"),
            PathopError::ProcessPathUpdate {
                user_path_added,
                reason,
            } => {
                write!(f, "pathop: failed to update process PATH: {reason}")?;
                if *user_path_added {
                    write!(f, " (user PATH was updated)")?;
                }
                Ok(())
            }
        }
    }
}
impl std::error::Error for PathopError {}

/// 注册表值类型（EXPAND_SZ 含 %VAR% 引用，Sz 纯字符串）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    ExpandSz,
    Sz,
}

/// Windows 后端注入缝（D4-10）。生产 M0 `RealBackend` 接注册表 FFI + std::env + 广播；
/// 测试用内存替身。abs = cleanAbsDir（Abs+Clean）。
pub trait WindowsBackend {
    fn abs(&self, dir: &str) -> Result<String, PathopError>;
    /// None = 值不存在（registry.ErrNotExist）。
    fn read_user_path(&self) -> Result<Option<(String, ValueKind)>, PathopError>;
    fn write_user_path(&mut self, value: &str, kind: ValueKind) -> Result<(), PathopError>;
    /// Read the original process PATH, including non-Unicode values. None means unset.
    fn process_path(&self) -> Option<OsString>;
    fn set_process_path(&mut self, value: &str) -> std::io::Result<()>;
    fn env(&self, name: &str) -> Option<String>;
    fn broadcast(&mut self);
}

// ---- 规范化（Go filepath.Clean 词典端口，D4-11）----

/// POSIX Clean：空→"."，去 "."/空段，".." 弹栈（rooted 根处丢弃），折叠分隔符。
pub fn clean_posix(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let rooted = path.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for comp in path.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                if !out.is_empty() && *out.last().unwrap() != ".." {
                    out.pop();
                } else if !rooted {
                    out.push("..");
                }
            }
            c => out.push(c),
        }
    }
    let joined = out.join("/");
    if rooted {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

/// Windows Clean：'/'→'\\'，保留驱动器卷 "C:"（"C:"→"C:."），UNC 前导 "\\\\" 保留。
/// 天花板：仅保留简单 UNC 前导；不保护 host/share 免受 ".." 弹栈，exotic UNC 延后 M3。
pub fn clean_windows(path: &str) -> String {
    let p = path.replace('/', "\\");
    if p.is_empty() {
        return ".".to_string();
    }
    let bytes = p.as_bytes();
    let vol_end = if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        2
    } else {
        0
    };
    let vol = &p[..vol_end];
    let rest = &p[vol_end..];
    let rooted = rest.starts_with('\\');
    let unc = vol_end == 0 && p.starts_with("\\\\");
    let mut out: Vec<&str> = Vec::new();
    for comp in rest.split('\\') {
        match comp {
            "" | "." => {}
            ".." => {
                if !out.is_empty() && *out.last().unwrap() != ".." {
                    out.pop();
                } else if !rooted && !unc {
                    out.push("..");
                }
            }
            c => out.push(c),
        }
    }
    let joined = out.join("\\");
    if unc {
        format!("\\\\{joined}")
    } else if rooted {
        format!("{vol}\\{joined}")
    } else if joined.is_empty() {
        if vol.is_empty() {
            ".".to_string()
        } else {
            format!("{vol}.")
        }
    } else {
        format!("{vol}{joined}")
    }
}

// ---- expand_env（Go os.ExpandEnv $-风格，非 %VAR%，D4-11）----

fn is_alnum(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// Go isShellSpecialVar：* # $ @ ! ? - 0-9。
fn is_shell_special_var(c: u8) -> bool {
    matches!(
        c,
        b'*' | b'#' | b'$' | b'@' | b'!' | b'?' | b'-' | b'0'..=b'9'
    )
}

/// Go getShellName 逐字端口：${name}（花括号内扫到 '}'，**无 alnum 限制**）/
/// ${special}（len>2 && special && s[2]=='}' → w3）/ $special（w1）/ $name（alnum+_）。
fn shell_name(s: &str) -> (&str, usize) {
    let b = s.as_bytes();
    if b.is_empty() {
        return ("", 0);
    }
    if b[0] == b'{' {
        if b.len() > 2 && is_shell_special_var(b[1]) && b[2] == b'}' {
            return (&s[1..2], 3);
        }
        for i in 1..b.len() {
            if b[i] == b'}' {
                if i == 1 {
                    return ("", 2);
                } // bad syntax：eat "${}"
                return (&s[1..i], i + 1);
            }
        }
        return ("", 1); // bad syntax：eat "${"
    }
    if is_shell_special_var(b[0]) {
        return (&s[0..1], 1);
    }
    let mut i = 0;
    while i < b.len() && is_alnum(b[i]) {
        i += 1;
    }
    (&s[..i], i)
}

/// Go os.Expand 端口。缺变量 → get 返回 ""（调用方传 unwrap_or_default）。
/// lone '$' 保留；invalid syntax 吃字符；%VAR% 字面不动（$-风格）。
pub fn expand_env_with(s: &str, get: impl Fn(&str) -> String) -> String {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0; // pending 字面起点
    let mut j = 0;
    while j < b.len() {
        if b[j] == b'$' && j + 1 < b.len() {
            out.push_str(&s[i..j]);
            let (name, w) = shell_name(&s[j + 1..]);
            if name.is_empty() && w > 0 {
                // invalid syntax：吃字符（不输出）
            } else if name.is_empty() {
                out.push('$'); // 合法语法但 $ 后无名 → 保留 $
            } else {
                out.push_str(&get(name));
            }
            j += w;
            i = j + 1;
        }
        j += 1;
    }
    out.push_str(&s[i..]);
    out
}

// ---- path_contains_dir（cfg 平台分隔符/clean）----

#[cfg(windows)]
const SEP: char = ';';
#[cfg(not(windows))]
const SEP: char = ':';
#[cfg(windows)]
fn clean(p: &str) -> String {
    clean_windows(p)
}
#[cfg(not(windows))]
fn clean(p: &str) -> String {
    clean_posix(p)
}

/// env_path 是否含 dir（clean 双方，trim，空段跳过）。fold = 大小写不敏感。
/// 天花板：fold 用 eq_ignore_ascii_case（Go EqualFold 是 Unicode 简单折叠；PATH 实际 ASCII）。
pub fn path_contains_dir(env_path: &str, dir: &str, fold: bool) -> bool {
    let dir = clean(dir);
    for p in env_path.split(SEP) {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        let cleaned = clean(p);
        if fold {
            if cleaned.eq_ignore_ascii_case(&dir) {
                return true;
            }
        } else if cleaned == dir {
            return true;
        }
    }
    false
}

/// Windows 语义版本（始终 ';' + clean_windows + fold），供 Windows 纯算法跨平台测试。
fn path_contains_dir_windows(env_path: &str, dir: &str) -> bool {
    let dir = clean_windows(dir);
    for p in env_path.split(';') {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        if clean_windows(p).eq_ignore_ascii_case(&dir) {
            return true;
        }
    }
    false
}

/// is_in_path 纯核心（env_path 注入）。Windows：cleanAbsDir + fold；非 Windows：fold=false。
pub fn is_in_path_with(env_path: &str, dir: &str) -> bool {
    #[cfg(windows)]
    {
        match std::path::absolute(dir) {
            Ok(abs) => path_contains_dir(env_path, &clean_windows(&abs.to_string_lossy()), true),
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    {
        path_contains_dir(env_path, dir, false)
    }
}

/// 薄包装：读真实进程 PATH。
pub fn is_in_path(dir: &str) -> bool {
    is_in_path_with(&std::env::var("PATH").unwrap_or_default(), dir)
}

/// 用户永久 PATH 是否含 abs_dir。先匹配字面路径，再尝试展开 $VAR。
pub fn user_path_contains_with(
    current_path: &str,
    abs_dir: &str,
    get: impl Fn(&str) -> String,
) -> bool {
    for p in current_path.split(';') {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        if clean_windows(p).eq_ignore_ascii_case(abs_dir)
            || (p.contains('$')
                && clean_windows(&expand_env_with(p, &get)).eq_ignore_ascii_case(abs_dir))
        {
            return true;
        }
    }
    false
}

// ---- Windows 纯算法（可注入替身，跨平台测试）----

// Documented SetEnvironmentVariableW limit for the variable value.
const MAX_PROCESS_PATH_UTF16: usize = 32_767;

fn process_path_update(
    abs_dir: &str,
    current: Option<OsString>,
) -> Result<Option<String>, PathopError> {
    let current = current
        .unwrap_or_default()
        .into_string()
        .map_err(|_| PathopError::Backend("process PATH contains non-Unicode text".into()))?;
    if current.contains('\0') {
        return Err(PathopError::Backend("process PATH contains a NUL".into()));
    }
    if path_contains_dir_windows(&current, abs_dir) {
        return Ok(None);
    }
    let new = if current.is_empty() {
        abs_dir.to_owned()
    } else {
        format!("{current};{abs_dir}")
    };
    if new.encode_utf16().count() > MAX_PROCESS_PATH_UTF16 {
        return Err(PathopError::Backend(format!(
            "process PATH would exceed {MAX_PROCESS_PATH_UTF16} UTF-16 code units"
        )));
    }
    Ok(Some(new))
}

/// Add the directory to the Windows user PATH and update the process PATH.
/// Returns true when it adds a user entry. If the process update fails after
/// the user write, keep the persisted entry and broadcast the change if requested.
/// The error tells callers whether a user entry was added by this attempt.
pub fn ensure_in_path_windows(
    dir: &str,
    with_broadcast: bool,
    b: &mut dyn WindowsBackend,
) -> Result<bool, PathopError> {
    let abs_dir = b.abs(dir)?;
    if abs_dir.contains(';') {
        return Err(PathopError::Backend(
            "directory contains a semicolon and cannot be added as one PATH entry".into(),
        ));
    }
    if abs_dir.contains('\0') {
        return Err(PathopError::Backend("directory contains a NUL".into()));
    }
    let (current_path, val_type) = match b.read_user_path()? {
        Some(v) => v,
        None => (String::new(), ValueKind::ExpandSz), // ErrNotExist → "" + EXPAND_SZ
    };
    let process_update = process_path_update(&abs_dir, b.process_path())?;
    let added = !user_path_contains_with(&current_path, &abs_dir, |n| b.env(n).unwrap_or_default());
    if added {
        let new_path = if current_path.is_empty() {
            abs_dir
        } else if current_path.ends_with(';') {
            format!("{current_path}{abs_dir}")
        } else {
            format!("{current_path};{abs_dir}")
        };
        b.write_user_path(&new_path, val_type)?;
    }
    let process_result = process_update.map_or(Ok(()), |path| b.set_process_path(&path));
    if added && with_broadcast {
        b.broadcast();
    }
    process_result.map_err(|error| PathopError::ProcessPathUpdate {
        user_path_added: added,
        reason: error.to_string(),
    })?;
    Ok(added)
}

/// IsInUserPATH 纯核心。
pub fn is_in_user_path_windows(dir: &str, b: &dyn WindowsBackend) -> Result<bool, PathopError> {
    let abs_dir = b.abs(dir)?;
    let current = match b.read_user_path()? {
        Some((v, _)) => v,
        None => String::new(),
    };
    Ok(user_path_contains_with(&current, &abs_dir, |n| {
        b.env(n).unwrap_or_default()
    }))
}

// ---- 真实 Windows 后端（cfg(windows)，std-only FFI，D4-10）----
// mac/linux 门禁不编译此段；正确性靠 Windows CI 首验。测试用替身，绝不触真实注册表/PATH。

#[cfg(any(windows, test))]
const REG_SZ: u32 = 1;
#[cfg(any(windows, test))]
const REG_EXPAND_SZ: u32 = 2;

#[cfg(any(windows, test))]
fn decode_registry_path(typ: u32, buf: &[u8], cb: u32) -> Result<(String, ValueKind), PathopError> {
    let kind = match typ {
        REG_SZ => ValueKind::Sz,
        REG_EXPAND_SZ => ValueKind::ExpandSz,
        _ => {
            return Err(PathopError::Backend(format!(
                "registry PATH type {typ} is not a string"
            )));
        }
    };
    let data = buf
        .get(..cb as usize)
        .filter(|data| data.len().is_multiple_of(2))
        .ok_or_else(|| PathopError::Backend("registry PATH has an invalid byte count".into()))?;
    let units: Vec<u16> = data
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    // RegQueryValueExW can return a string without its terminating NUL.
    let units = units.strip_suffix(&[0]).unwrap_or(&units);
    if units.contains(&0) {
        return Err(PathopError::Backend(
            "registry PATH contains an interior NUL".into(),
        ));
    }
    let s = String::from_utf16(units)
        .map_err(|_| PathopError::Backend("registry PATH contains invalid UTF-16".into()))?;
    Ok((s, kind))
}

#[cfg(windows)]
mod win {
    use super::{
        OsString, PathopError, REG_EXPAND_SZ, REG_SZ, ValueKind, WindowsBackend, clean_windows,
        decode_registry_path,
    };

    // Windows headers cast this predefined handle through signed LONG before pointer width.
    const HKEY_CURRENT_USER: isize = 0x80000001u32 as i32 as isize;
    const KEY_QUERY_VALUE: u32 = 0x0001;
    const KEY_SET_VALUE: u32 = 0x0002;
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const HWND_BROADCAST: isize = 0xFFFF;
    const WM_SETTINGCHANGE: u32 = 0x001A;
    const SMTO_ABORTIFHUNG: u32 = 0x0002;

    // edition 2024：extern 块须 `unsafe extern`（unsafe-extern）；unsafe fn 内每个原生
    // 调用须显式 `unsafe {}`（unsafe_op_in_unsafe_fn edition 2024 默认 warn，经 -D warnings 升级为错误）。
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegOpenKeyExW(
            hkey: isize,
            subkey: *const u16,
            options: u32,
            sam: u32,
            result: *mut isize,
        ) -> i32;
        fn RegQueryValueExW(
            hkey: isize,
            name: *const u16,
            reserved: *mut u32,
            typ: *mut u32,
            data: *mut u8,
            cb: *mut u32,
        ) -> i32;
        fn RegSetValueExW(
            hkey: isize,
            name: *const u16,
            reserved: u32,
            typ: u32,
            data: *const u8,
            cb: u32,
        ) -> i32;
        fn RegCloseKey(hkey: isize) -> i32;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SendMessageTimeoutW(
            hwnd: isize,
            msg: u32,
            wparam: usize,
            lparam: *const u16,
            flags: u32,
            timeout: u32,
            result: *mut usize,
        ) -> isize;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetEnvironmentVariableW(name: *const u16, value: *const u16) -> i32;
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn read_path() -> Result<Option<(String, ValueKind)>, PathopError> {
        let subkey = to_wide("Environment"); // NUL 结尾 wide，调用期间存活
        let mut hkey: isize = 0;
        // SAFETY: subkey.as_ptr() 指 NUL 结尾 wide 串；&mut hkey 是合法 out 指针；HKCU 伪句柄常量。
        let rc = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_QUERY_VALUE,
                &mut hkey,
            )
        };
        if rc != 0 {
            return Err(PathopError::Backend(format!("RegOpenKeyEx: {rc}")));
        }
        let name = to_wide("Path");
        let mut size_type: u32 = 0;
        let mut cb: u32 = 0;
        // SAFETY: hkey 已由 RegOpenKeyExW 成功打开；name 合法；data=null 仅查询尺寸，cb/typ 为合法 out。
        let rc = unsafe {
            RegQueryValueExW(
                hkey,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut size_type,
                std::ptr::null_mut(),
                &mut cb,
            )
        };
        if rc == ERROR_FILE_NOT_FOUND {
            // SAFETY: hkey was successfully opened above and has not been closed.
            unsafe { RegCloseKey(hkey) };
            return Ok(None);
        }
        if rc != 0 {
            // SAFETY: hkey was successfully opened above and has not been closed.
            unsafe { RegCloseKey(hkey) };
            return Err(PathopError::Backend(format!("RegQueryValueEx size: {rc}")));
        }
        if size_type != REG_SZ && size_type != REG_EXPAND_SZ {
            // SAFETY: hkey was successfully opened above and has not been closed.
            unsafe { RegCloseKey(hkey) };
            return Err(PathopError::Backend(format!(
                "registry PATH type {size_type} is not a string"
            )));
        }
        let mut buf = vec![0u8; cb as usize]; // 尺寸由首次查询返回，buf 容量 = cb
        let mut read_type = 0;
        // SAFETY: buf.as_mut_ptr() 可写 cb 字节（vec 容量 cb）；hkey 仍有效；cb 为合法 in/out。
        let rc = unsafe {
            RegQueryValueExW(
                hkey,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut read_type,
                buf.as_mut_ptr(),
                &mut cb,
            )
        };
        // SAFETY: hkey was successfully opened above and has not been closed.
        unsafe { RegCloseKey(hkey) };
        if rc != 0 {
            return Err(PathopError::Backend(format!("RegQueryValueEx read: {rc}")));
        }
        decode_registry_path(read_type, &buf, cb).map(Some)
    }

    unsafe fn write_path(value: &str, kind: ValueKind) -> Result<(), PathopError> {
        let subkey = to_wide("Environment");
        let mut hkey: isize = 0;
        // SAFETY: subkey NUL 结尾 wide；&mut hkey 合法 out；sam 含 QUERY|SET 以读改写。
        let rc = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
                &mut hkey,
            )
        };
        if rc != 0 {
            return Err(PathopError::Backend(format!("RegOpenKeyEx: {rc}")));
        }
        let name = to_wide("Path");
        let wide = to_wide(value); // 含尾 NUL，字节数 = wide.len()*2
        let typ = if kind == ValueKind::ExpandSz {
            REG_EXPAND_SZ
        } else {
            REG_SZ
        };
        // SAFETY: wide.as_ptr() 指 wide.len()*2 字节合法缓冲（含 NUL）；cb 与之精确匹配；hkey 有效。
        let rc = unsafe {
            RegSetValueExW(
                hkey,
                name.as_ptr(),
                0,
                typ,
                wide.as_ptr() as *const u8,
                (wide.len() * 2) as u32,
            )
        };
        // SAFETY: hkey was successfully opened above and has not been closed.
        unsafe { RegCloseKey(hkey) };
        if rc != 0 {
            return Err(PathopError::Backend(format!("RegSetValueEx: {rc}")));
        }
        Ok(())
    }

    unsafe fn broadcast() {
        let env = to_wide("Environment"); // lparam 指 "Environment" wide 串，调用期间存活
        let mut result: usize = 0;
        // SAFETY: HWND_BROADCAST/WM_SETTINGCHANGE 常量；env.as_ptr() NUL 结尾 wide；&mut result 合法 out；超时 2000ms。
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                env.as_ptr(),
                SMTO_ABORTIFHUNG,
                2000,
                &mut result,
            )
        };
    }

    pub struct RealBackend;
    impl WindowsBackend for RealBackend {
        fn abs(&self, dir: &str) -> Result<String, PathopError> {
            let a =
                std::path::absolute(dir).map_err(|e| PathopError::Backend(format!("abs: {e}")))?;
            Ok(clean_windows(&a.to_string_lossy()))
        }
        fn read_user_path(&self) -> Result<Option<(String, ValueKind)>, PathopError> {
            // SAFETY: read_path owns its buffers and opens/closes its own registry handle.
            unsafe { read_path() }
        }
        fn write_user_path(&mut self, value: &str, kind: ValueKind) -> Result<(), PathopError> {
            // SAFETY: write_path owns the encoded value and its registry handle.
            unsafe { write_path(value, kind) }
        }
        fn process_path(&self) -> Option<OsString> {
            std::env::var_os("PATH")
        }
        fn set_process_path(&mut self, value: &str) -> std::io::Result<()> {
            let name = to_wide("PATH");
            let value = to_wide(value);
            // SAFETY: both pointers are NUL-terminated UTF-16 buffers live for the call.
            if unsafe { SetEnvironmentVariableW(name.as_ptr(), value.as_ptr()) } == 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        }
        fn env(&self, name: &str) -> Option<String> {
            std::env::var(name).ok()
        }
        fn broadcast(&mut self) {
            // SAFETY: broadcast owns the message buffer throughout the synchronous call.
            unsafe {
                broadcast();
            }
        }
    }
}

// ---- 公开平台 API（cfg）----

#[cfg(not(windows))]
pub fn is_in_user_path(_dir: &str) -> Result<bool, PathopError> {
    Ok(false)
}
#[cfg(not(windows))]
pub fn ensure_in_path(_dir: &str) -> Result<bool, PathopError> {
    Ok(false)
}
#[cfg(not(windows))]
pub fn ensure_in_path_with_broadcast(_dir: &str) -> Result<bool, PathopError> {
    Ok(false)
}

#[cfg(windows)]
pub fn is_in_user_path(dir: &str) -> Result<bool, PathopError> {
    is_in_user_path_windows(dir, &win::RealBackend)
}
#[cfg(windows)]
pub fn ensure_in_path(dir: &str) -> Result<bool, PathopError> {
    ensure_in_path_windows(dir, false, &mut win::RealBackend)
}
#[cfg(windows)]
pub fn ensure_in_path_with_broadcast(dir: &str) -> Result<bool, PathopError> {
    ensure_in_path_windows(dir, true, &mut win::RealBackend)
}

#[cfg(test)]
mod registry_tests {
    use super::{ValueKind, decode_registry_path};

    fn bytes(units: &[u16]) -> Vec<u8> {
        units.iter().flat_map(|unit| unit.to_le_bytes()).collect()
    }

    #[test]
    fn registry_path_uses_final_type_and_reported_bytes() {
        let data = bytes(&[0x754c, 0xd83d, 0xde00, 0, 0xd800]);
        assert_eq!(
            decode_registry_path(1, &data, 8).unwrap(),
            ("界😀".to_string(), ValueKind::Sz)
        );
        assert_eq!(
            decode_registry_path(2, &data, 6).unwrap(),
            ("界😀".to_string(), ValueKind::ExpandSz)
        );
        assert_eq!(
            decode_registry_path(1, &[], 0).unwrap(),
            (String::new(), ValueKind::Sz)
        );
        assert!(decode_registry_path(3, &data, 8).is_err());
    }

    #[test]
    fn registry_path_rejects_impossible_byte_counts() {
        let data = bytes(&[0x0061, 0]);
        assert!(decode_registry_path(1, &data, 3).is_err());
        assert!(decode_registry_path(1, &data, 6).is_err());
    }

    #[test]
    fn registry_path_rejects_unrepresentable_text() {
        for units in [&[0xd800, 0][..], &[0x0061, 0, 0x0062], &[0x0061, 0, 0]] {
            let data = bytes(units);
            assert!(
                decode_registry_path(1, &data, data.len() as u32).is_err(),
                "accepted {units:?}"
            );
        }
    }
}
