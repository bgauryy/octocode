use super::*;

#[test]
fn typed_facts_preserve_ranges_modules_edges_and_commonjs() {
    let json = r#"{
          "kind":"graphFacts","schemaVersion":1,"source":"native-ast","language":"rust","file":"lib.rs",
          "declarations":[{"id":"d","name":"run","kind":"function","line":1,"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":8}},"selectionRange":{"start":{"line":0,"character":3},"end":{"line":0,"character":6}},"exported":true}],
          "edges":[{"id":"e","from":"d","to":"c","relation":"calls","source":"native-ast","line":1,"resolution":"unresolved"}],
          "calls":[{"id":"c","caller":"d","callee":"work","line":1,"range":{"start":{"line":0,"character":7},"end":{"line":0,"character":11}},"kind":"direct"}],
          "modules":[{"name":"child","line":2,"scope":[],"inline":false,"path":"child.rs","unsupported":false}],
          "commonJs":[{"specifier":"pkg","line":3,"kind":"commonjs-require","binding":"require"}]
        }"#;
    let facts: GraphFactsDocument = serde_json::from_str(json).expect("typed facts");
    assert_eq!(facts.declarations[0].selection_range.start.character, 3);
    assert_eq!(facts.edges[0].relation, "calls");
    assert_eq!(facts.modules[0].path.as_deref(), Some("child.rs"));
    assert_eq!(facts.common_js[0].specifier.as_deref(), Some("pkg"));
    assert_eq!(facts.common_js[0].kind, "commonjs-require");
}
