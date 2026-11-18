use super::*;
#[test]
fn comments_and_siblings_survive() {
    let s = "{ // keep\n\"lsp\":{\"prewarm\":\"off\",/*sibling*/\"autoInstall\":\"auto\",},\"other\":3}";
    let out = edit_config_json(s, &["lsp", "prewarm"], Some(&serde_json::json!("all"))).unwrap();
    assert!(out.contains("/*sibling*/"));
    assert!(out.contains("// keep"));
    assert_eq!(parse_config_json(&out).unwrap()["lsp"]["prewarm"], "all");
    let out = edit_config_json(&out, &["lsp", "autoInstall"], None).unwrap();
    assert!(out.contains("/*sibling*/"));
    assert_eq!(parse_config_json(&out).unwrap()["other"], 3);
}
#[test]
fn duplicate_keys_rejected() {
    assert!(parse_config_json("{\"a\":1,\"a\":2}").is_err());
    assert!(parse_config_json("{\"a\":[{\"b\":1,\"b\":2}]}").is_err());
}
#[test]
fn new_nested_member() {
    let out = edit_config_json(
        "{/*keep*/}",
        &["section", "child", "leaf"],
        Some(&serde_json::json!(4000)),
    )
    .unwrap();
    assert!(out.contains("/*keep*/"));
    assert_eq!(
        parse_config_json(&out).unwrap()["section"]["child"]["leaf"],
        4000
    );
}
