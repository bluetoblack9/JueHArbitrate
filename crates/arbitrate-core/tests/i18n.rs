//! core::i18n 契约测试 — en/zh 目录、英文兜底、迷你格式化器、品牌适配
//! （规格：上游 internal/i18n，Rust 自有重写，不运行 Go）。

use arbitrate_core::i18n::{
    Arg, Bundle, LANG_EN, LANG_ZH, MESSAGES_EN, MESSAGES_ZH, format_message, normalize,
};
use std::collections::BTreeMap;

#[test]
fn normalize_table() {
    let cases = [
        ("en", "en"),
        ("EN", "en"),
        (" en ", "en"),
        ("zh", "zh"),
        ("zh_CN", "zh"),
        ("zh-Hans", "zh"),
        ("zh.UTF-8", "zh"),
        ("EN_us", "en"),
        ("en-US", "en"),
        ("", ""),
        ("fr", ""),
        ("_weird", ""),
        ("fr_FR", ""),
        ("  ZH-Hant  ", "zh"),
    ];
    for (input, want) in cases {
        assert_eq!(normalize(input), want, "normalize({input:?})");
    }
}

#[test]
fn bundle_new_lang() {
    assert_eq!(Bundle::new("zh").lang(), LANG_ZH);
    assert_eq!(Bundle::new("ZH-CN").lang(), LANG_ZH);
    assert_eq!(Bundle::new("en").lang(), LANG_EN);
    assert_eq!(Bundle::new("fr").lang(), LANG_EN); // 不支持 → en
    assert_eq!(Bundle::new("").lang(), LANG_EN);
}

#[test]
fn t_fallback_chain() {
    let zh = Bundle::new("zh");
    assert_eq!(zh.t("view.starting", &[]), "启动中…");
    // 未知 key → 返回 key 本身（en 也没有）
    assert_eq!(zh.t("nope.missing", &[]), "nope.missing");
    let en = Bundle::new("en");
    assert_eq!(en.t("view.starting", &[]), "starting…");
}

#[test]
fn t_sparse_fallback_hole() {
    // 缺 key → en 兜底
    let mut sparse = BTreeMap::new();
    sparse.insert("view.thinking".to_string(), "思考中…".to_string());
    let b = Bundle::from_catalogue("zh", sparse);
    assert_eq!(b.t("view.starting", &[]), "starting…"); // en 兜底
    assert_eq!(b.t("view.thinking", &[]), "思考中…");
    // present-but-empty → 返回 key（不回退 en！上游 if s=="" 在兜底之后）
    let mut empty = BTreeMap::new();
    empty.insert("view.starting".to_string(), String::new());
    let b2 = Bundle::from_catalogue("zh", empty);
    assert_eq!(b2.t("view.starting", &[]), "view.starting");
}

#[test]
fn t_args_zh_en() {
    let zh = Bundle::new("zh");
    let en = Bundle::new("en");
    assert_eq!(zh.t("view.thinking_s", &[Arg::Int(7)]), "思考中… 7 秒");
    assert_eq!(
        zh.t("view.thinking_ms", &[Arg::Int(2), Arg::Int(5)]),
        "思考中… 2 分 5 秒"
    );
    assert_eq!(
        en.t("view.thinking_ms", &[Arg::Int(2), Arg::Int(5)]),
        "thinking… 2m5s"
    );
    assert_eq!(
        en.t("view.provider_banner", &[Arg::Str("x".into())]),
        "⚠ Provider: x — FIM / cache stats / Reasoner disabled"
    );
    assert_eq!(
        en.t("cmd.unknown", &[Arg::Str("/foo".into())]),
        "unknown command /foo — try /help"
    );
    assert_eq!(
        zh.t("lang.unsupported", &[Arg::Str("fr".into())]),
        "不支持的语言 \"fr\" —— 可选：en、zh。"
    );
    assert_eq!(
        en.t("lang.unsupported", &[Arg::Str("fr".into())]),
        "Unsupported language \"fr\" — supported: en, zh."
    );
}

#[test]
fn formatter_verbs() {
    assert_eq!(
        format_message("%s-%d", &[Arg::Str("a".into()), Arg::Int(3)]),
        "a-3"
    );
    assert_eq!(format_message("%v", &[Arg::Str("err".into())]), "err");
    assert_eq!(format_message("%v", &[Arg::Int(9)]), "9");
    assert_eq!(format_message("100%%", &[]), "100%");
    assert_eq!(format_message("no verbs", &[Arg::Int(1)]), "no verbs"); // 多余 arg 忽略
    assert_eq!(format_message("50%…", &[]), "50%…");
    assert_eq!(format_message("unknown %z verb", &[]), "unknown %z verb");
    assert_eq!(format_message("trailing %", &[]), "trailing %");
}

#[test]
fn formatter_quote() {
    assert_eq!(format_message("%q", &[Arg::Str("fr".into())]), "\"fr\"");
    assert_eq!(
        format_message("%q", &[Arg::Str("a\nb".into())]),
        "\"a\\nb\""
    );
    assert_eq!(
        format_message("%q", &[Arg::Str("a\"b".into())]),
        "\"a\\\"b\""
    );
    assert_eq!(
        format_message("%q", &[Arg::Str("a\\b".into())]),
        "\"a\\\\b\""
    );
    assert_eq!(
        format_message("%q", &[Arg::Str("t\th".into())]),
        "\"t\\th\""
    );
    assert_eq!(
        format_message("%q", &[Arg::Str("\u{1}".into())]),
        "\"\\x01\""
    );
    assert_eq!(format_message("%q", &[Arg::Str("中".into())]), "\"中\""); // 可打印非 ASCII 原样
}

#[test]
fn formatter_type_mismatch() {
    // Go fmt 兼容错误标记
    assert_eq!(format_message("%s", &[Arg::Int(7)]), "%!s(int=7)");
    assert_eq!(
        format_message("%d", &[Arg::Str("x".into())]),
        "%!d(string=x)"
    );
    assert_eq!(format_message("%q", &[Arg::Int(7)]), "%!q(int=7)");
}

#[test]
fn formatter_missing_extra() {
    assert_eq!(
        format_message("%s and %s", &[Arg::Str("a".into())]),
        "a and %!s(MISSING)"
    );
    assert_eq!(format_message("%d", &[]), "%!d(MISSING)");
    // 多余 arg 忽略（文档化偏差，Go 追加 EXTRA）
    assert_eq!(
        format_message("%s", &[Arg::Str("a".into()), Arg::Int(2)]),
        "a"
    );
}

#[test]
fn catalogue_parity() {
    let en: BTreeMap<_, _> = MESSAGES_EN.iter().copied().collect();
    let zh: BTreeMap<_, _> = MESSAGES_ZH.iter().copied().collect();
    assert_eq!(en.len(), 14);
    assert_eq!(zh.len(), 14);
    assert_eq!(
        en.keys().collect::<Vec<_>>(),
        zh.keys().collect::<Vec<_>>(),
        "en/zh key 集合必须一致"
    );
}

#[test]
fn placeholder_signature_parity() {
    fn verbs(s: &str) -> String {
        let b = s.as_bytes();
        let mut out = String::new();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'%' && i + 1 < b.len() {
                let v = b[i + 1] as char;
                if v == '%' {
                    i += 2;
                    continue;
                }
                out.push(v);
                i += 2;
            } else {
                i += 1;
            }
        }
        out
    }
    let zh: BTreeMap<_, _> = MESSAGES_ZH.iter().copied().collect();
    fn expected_signature(key: &str) -> &'static str {
        match key {
            "view.starting" => "",
            "view.provider_banner" => "s",
            "view.thinking" => "",
            "view.thinking_s" => "d",
            "view.thinking_ms" => "dd",
            "view.queue.steering" => "",
            "view.queue.queued" => "",
            "menu.hint.simple" => "",
            "menu.hint.toggle" => "",
            "cmd.unknown" => "s",
            "lang.status" => "s",
            "lang.switched" => "s",
            "lang.unsupported" => "q",
            "lang.persist_failed" => "v",
            _ => panic!("unexpected catalogue key: {key}"),
        }
    }
    for (key, en_val) in MESSAGES_EN {
        let zh_val = zh.get(key).unwrap();
        let expected = expected_signature(key);
        for verb in verbs(en_val).chars().chain(verbs(zh_val).chars()) {
            assert!(
                matches!(verb, 's' | 'd' | 'q' | 'v'),
                "key {key} 含不支持的占位符 %{verb}"
            );
        }
        assert_eq!(verbs(en_val), expected, "key {key} 英文占位符签名");
        assert_eq!(verbs(zh_val), expected, "key {key} 中文占位符签名");
    }
}

#[test]
fn brand_adaptation() {
    let en = Bundle::new("en");
    let zh = Bundle::new("zh");
    // 唯一 i18n 路径适配：~/.seek → ~/.arbitrate
    assert_eq!(
        en.t("lang.persist_failed", &[Arg::Str("disk full".into())]),
        "warning: could not persist language to ~/.arbitrate/config.json: disk full"
    );
    assert_eq!(
        zh.t("lang.persist_failed", &[Arg::Str("disk full".into())]),
        "警告：语言设置未能写入 ~/.arbitrate/config.json：disk full"
    );
    // SEEK_LANG 保留（约束 10 禁全局替换）
    assert!(
        en.t("lang.status", &[Arg::Str("en".into())])
            .contains("SEEK_LANG")
    );
    assert!(
        zh.t("lang.status", &[Arg::Str("zh".into())])
            .contains("SEEK_LANG")
    );
}

#[test]
fn translation_smoke() {
    // 所有目录值渲染后不得含 Go fmt 错误标记 "%!"
    for bundle in [Bundle::new("en"), Bundle::new("zh")] {
        for (key, _) in MESSAGES_EN {
            let args = match *key {
                "view.provider_banner" => vec![Arg::Str("provider-x".into())],
                "view.thinking_s" => vec![Arg::Int(7)],
                "view.thinking_ms" => vec![Arg::Int(2), Arg::Int(5)],
                "cmd.unknown" => vec![Arg::Str("/unknown".into())],
                "lang.status" => vec![Arg::Str("en".into())],
                "lang.switched" => vec![Arg::Str("zh".into())],
                "lang.unsupported" => vec![Arg::Str("fr".into())],
                "lang.persist_failed" => vec![Arg::Str("disk full".into())],
                _ => vec![],
            };
            let rendered = bundle.t(key, &args);
            assert!(
                !rendered.contains("%!"),
                "key {key} 渲染含错误标记: {rendered}"
            );
        }
    }
}
