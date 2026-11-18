use super::*;
use octocode_test_support::json_fixture;

/// A label with a signature (jdtls) anchors by its name: the name fed
/// back as `symbolName` resolves, and the signature stays in `detail`.
#[test]
fn a_signature_label_is_named_by_its_identifier() {
    let at = json_fixture(
        r#"{
                "start": {"line": 3, "character": 25},
                "end": {"line": 3, "character": 30}
            }"#,
    );
    let node = |name: &str, detail: Value| {
        json!({"name": name, "kind": 6, "detail": detail, "uri": "file:///w/Use.java",
                "range": at, "selectionRange": at})
    };
    let java = public_hierarchy_node(&node("useIt(Port) : String", json!("demo.Use")));
    assert_eq!(java["symbolName"], "useIt", "{java}");
    assert_eq!(java["detail"], "demo.Use", "{java}");
    assert_eq!(java[SIGNATURE], "useIt(Port) : String", "{java}");
    let bare = public_hierarchy_node(&node("read(int)", json!("")));
    assert_eq!(bare["symbolName"], "read", "{bare}");
    assert_eq!(bare.get("detail"), None, "{bare}");
    assert_eq!(bare[SIGNATURE], "read(int)", "{bare}");
    assert_eq!(
        public_via(&node("useIt(Port) : String", json!("demo.Use")))["symbolName"],
        "useIt"
    );
    // Any other label is the name it always was.
    for label in ["read", "operator()", "(*T).Method", "Foo::bar"] {
        let plain = public_hierarchy_node(&node(label, json!("d")));
        assert_eq!(plain["symbolName"], label, "{plain}");
        assert_eq!(plain["detail"], "d", "{plain}");
    }
    assert_eq!(
        signed_detail(Some("demo.Use"), Some("useIt(Port) : String")).as_deref(),
        Some("demo.Use useIt(Port) : String")
    );
    let call = public_hierarchy_node(&node("operator()(int)", json!("")));
    assert_eq!(call["symbolName"], "operator()(int)", "{call}");
}
