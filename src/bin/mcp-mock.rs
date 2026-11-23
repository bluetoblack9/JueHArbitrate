// 测试辅助 bin：脚本驱动 MCP stdio 对端；产品构建/安装必须 --bin arbitrate。
use std::io::{BufRead, Write};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: mcp-mock <script.json>");
    let script: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let req: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
        let Some(id) = req.get("id") else { continue }; // notification 不回包。
        let method = req["method"].as_str().unwrap_or("");
        let resp = if let Some(entry) = script
            .iter()
            .find(|e| e["match_method"].as_str() == Some(method))
        {
            if entry["stall"].as_bool() == Some(true) {
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                }
            }
            let mut resp = entry.get("respond").expect("脚本条目缺 respond").clone();
            assert!(resp.is_object(), "respond 必须为 JSON 对象");
            resp["id"] = id.clone();
            resp
        } else {
            serde_json::json!({"jsonrpc":"2.0", "id":id, "error":{"code":-32601, "message":"Method not found"}})
        };
        writeln!(stdout, "{resp}").unwrap();
        stdout.flush().unwrap();
    }
}
