use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

struct Payload {
    name: String,
    original: Vec<u8>,
    staged: Vec<u8>,
    checked_out: Vec<u8>,
}

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "arbitrate-checkout-{}-{stamp}-{n}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git(repo: &Path, empty_config: &Path, args: &[&str]) -> Vec<u8> {
    let out = Command::new("git")
        .current_dir(repo)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_CONFIG_COUNT")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", empty_config)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

fn checkout_payloads(protected: bool) -> Vec<Payload> {
    let scratch = Scratch::new();
    let repo = scratch.0.join("repo");
    let checked = scratch.0.join("checked");
    let empty_config = scratch.0.join("empty.gitconfig");
    std::fs::create_dir(&repo).unwrap();
    std::fs::create_dir(&checked).unwrap();
    std::fs::write(&empty_config, b"").unwrap();
    git(&repo, &empty_config, &["init", "--quiet"]);
    git(
        &repo,
        &empty_config,
        &["config", "--local", "core.autocrlf", "true"],
    );
    git(
        &repo,
        &empty_config,
        &["config", "--local", "core.safecrlf", "false"],
    );
    let product = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if protected {
        std::fs::copy(product.join(".gitattributes"), repo.join(".gitattributes")).unwrap();
    }
    let mut payloads = Vec::new();
    for name in [
        "assets/webfrontend.html",
        "assets/LICENSE",
        "assets/NOTICE",
        "assets/builtin-skills/code-review.md",
        "assets/builtin-skills/dual-model.md",
        "assets/builtin-skills/go-test-runner.md",
        "assets/builtin-skills/plan-mode.md",
        "tests/fixtures/checkout-probe.json",
    ] {
        let bytes = if name.ends_with("checkout-probe.json") {
            b"{}\n".to_vec()
        } else {
            std::fs::read(product.join(name)).unwrap()
        };
        let target = repo.join(name);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, &bytes).unwrap();
        payloads.push((name.to_string(), bytes));
    }
    git(&repo, &empty_config, &["add", "--all"]);
    let prefix = format!("{}/", checked.to_string_lossy().replace('\\', "/"));
    git(
        &repo,
        &empty_config,
        &["checkout-index", "--all", &format!("--prefix={prefix}")],
    );
    payloads
        .into_iter()
        .map(|(name, original)| {
            let staged = git(&repo, &empty_config, &["show", &format!(":{name}")]);
            let checked_out = std::fs::read(checked.join(&name)).unwrap();
            Payload {
                name,
                original,
                staged,
                checked_out,
            }
        })
        .collect()
}

#[test]
fn without_attributes_probe_changes_to_crlf() {
    let files = checkout_payloads(false);
    let probe = files
        .iter()
        .find(|p| p.name.ends_with("checkout-probe.json"))
        .unwrap();
    assert_eq!(probe.original, b"{}\n");
    assert_eq!(probe.staged, probe.original);
    assert_eq!(probe.checked_out, b"{}\r\n");
}

#[test]
fn protected_staging_and_checkout_preserve_every_byte() {
    for p in checkout_payloads(true) {
        assert_eq!(p.staged, p.original, "暂存改写字节: {}", p.name);
        assert_eq!(p.checked_out, p.original, "检出改写字节: {}", p.name);
        assert_eq!(Sha256::digest(&p.checked_out), Sha256::digest(&p.original));
    }
}
