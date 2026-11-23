// 脚手架仅无参数启动输出版本；真实 CLI 分发在 M2。
fn main() {
    if std::env::args_os().nth(1).is_some() {
        eprintln!("arbitrate: not yet implemented");
        std::process::exit(1);
    }
    println!("arbitrate {}", env!("CARGO_PKG_VERSION"));
}
