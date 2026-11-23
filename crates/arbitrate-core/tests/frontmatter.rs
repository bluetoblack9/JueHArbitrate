use arbitrate_core::frontmatter::{FrontmatterError, split};

#[test]
fn scalar_and_quoted() {
    let s = split(
        b"---\nname: my-thing\ndescription: \"has: colon\"\nmodel: 'glm'\n---\n\nBody here.\n",
    )
    .unwrap();
    assert_eq!(s.scalars["name"], "my-thing");
    assert_eq!(s.scalars["description"], "has: colon");
    assert_eq!(s.scalars["model"], "glm");
    assert!(s.lists.is_empty());
    assert_eq!(s.body, "Body here.\n");
}

#[test]
fn list_shapes() {
    let s = split(b"---\ntools: [read, \"grep\", bash]\nsteps:\n  - one\n  - two\n---\nx").unwrap();
    assert_eq!(s.lists["tools"], ["read", "grep", "bash"]);
    assert_eq!(s.lists["steps"], ["one", "two"]);
    assert!(!s.scalars.contains_key("tools"));
}

#[test]
fn block_scalar() {
    let s = split(b"---\nnote: |\n  line 1\n  line 2\n---\n").unwrap();
    assert_eq!(s.scalars["note"], "line 1\nline 2");
    // |- 变体 + 内部空行保留 + 尾部空行剥掉
    let s = split(b"---\nnote: |-\n  a\n\n  b\n\n---\n").unwrap();
    assert_eq!(s.scalars["note"], "a\n\nb");
    // 缩进回落即止，后续顶层键正常解析
    let s = split(b"---\nnote: |\n  x\nnext: y\n---\n").unwrap();
    assert_eq!(s.scalars["note"], "x");
    assert_eq!(s.scalars["next"], "y");
    // 首个续行无缩进 → 空串
    let s = split(b"---\nnote: |\nnext: y\n---\n").unwrap();
    assert_eq!(s.scalars["note"], "");
    assert_eq!(s.scalars["next"], "y");
}

#[test]
fn structural_errors() {
    assert_eq!(
        split(b"# just markdown\n").unwrap_err(),
        FrontmatterError::NoFrontmatter
    );
    assert_eq!(
        split(b"---\nname: x\n").unwrap_err(),
        FrontmatterError::Unterminated
    );
    assert_eq!(
        split(b"---").unwrap_err(),
        FrontmatterError::EmptyAfterOpener
    );
}

#[test]
fn bom_and_blank_tolerance() {
    let s = split("\u{feff}\n\n---\nname: bom\n---\nbody".as_bytes()).unwrap();
    assert_eq!(s.scalars["name"], "bom");
    assert_eq!(s.body, "body");
}

#[test]
fn skill_dialect_parity() {
    let header = "---\nname: verifier\ndescription: Runs tests\nkeywords: [a, b]\nallowed-tools:\n  - read\n  - bash\nmulti: |\n  one\n  two\n# comment\n---\nrole prompt";
    let s = split(header.as_bytes()).unwrap();
    assert_eq!(s.scalars["name"], "verifier");
    assert_eq!(s.scalars["description"], "Runs tests");
    assert_eq!(s.scalars["multi"], "one\ntwo");
    assert_eq!(s.lists["keywords"], ["a", "b"]);
    assert_eq!(s.lists["allowed-tools"], ["read", "bash"]);
    assert_eq!(s.body, "role prompt");
}

#[test]
fn empty_value_and_unknown_lines() {
    let s =
        split(b"---\nempty:\n# comment\nnot a key value line\n  indented: stray\nkey: v\n---\nB")
            .unwrap();
    assert_eq!(s.scalars["empty"], "");
    assert_eq!(s.scalars["key"], "v");
    assert!(!s.scalars.contains_key("indented"));
    assert_eq!(s.scalars.len(), 2);
}

#[test]
fn crlf_tolerance() {
    let s = split(b"---\r\nname: crlf\r\n---\r\nbody\r\n").unwrap();
    assert_eq!(s.scalars["name"], "crlf");
    assert_eq!(s.body, "body\r\n");
}

#[test]
fn block_list_blank_between_items() {
    let s = split(b"---\nxs:\n  - a\n\n  - b\n---\nB").unwrap();
    assert_eq!(s.lists["xs"], ["a", "b"]);
}
