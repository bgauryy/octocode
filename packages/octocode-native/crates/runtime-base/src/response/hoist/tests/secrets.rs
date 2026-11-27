//! Every hoist form runs on what the output gate already sealed, or builds
//! text the gate then judges whole, so no form can carry a credential past
//! it. The stage edits a sealed envelope (`Sanitized::edit`, which in tests
//! asserts the edited envelope is still a fixed point of the gate); groups
//! and outline strings are built by the tools before the gate judges their
//! rows. Each case puts a token in the value a form hoists, merges or joins.
use super::*;
use crate::security::Sanitized;

/// A token the gate redacts wherever it stands.
fn token() -> String {
    format!("ghp_{}", "a1B2".repeat(9))
}

/// `envelope` through the gate, then the stage's hoist on the sealed value
/// (the edit asserts the gate's fixed point).
fn hoisted_after_the_gate(tool: ToolId, envelope: Value) -> Value {
    let mut sealed = Sanitized::sealed(envelope);
    sealed.edit(|structured| apply(tool, structured));
    sealed.into_value()
}

fn leaks(value: &Value) -> bool {
    value.to_string().contains("ghp_")
}

/// F1: a token every item shares moves to the holder already redacted.
#[test]
fn a_holder_moves_only_what_the_gate_sealed() {
    let file =
        |path: &str| json!({"path": path, "owner": token(), "repo": "r", "matches": ["1\tx"]});
    let stated = hoisted_after_the_gate(
        ToolId::GhSearchCode,
        json!({"results": [{"index": 0, "data": {"files": [file("a.rs"), file("b.rs")]}}]}),
    );
    assert!(!leaks(&stated), "{stated}");
    let data = &stated["results"][0]["data"];
    assert!(data["owner"].is_string(), "the holder states it: {stated}");
    assert!(data["files"][0].get("owner").is_none(), "{stated}");
}

/// F4: a shared identity, the hints of empty rows and the merged related
/// call hold the gate's text, never the token.
#[test]
fn the_batch_states_once_only_what_the_gate_sealed() {
    let shared = hoisted_after_the_gate(
        ToolId::GhStructure,
        json!({"results": [
            {"index": 0, "data": {"ref": token(), "files": []}},
            {"index": 1, "data": {"ref": token(), "files": []}},
        ]}),
    );
    assert!(!leaks(&shared), "{shared}");
    assert!(shared.pointer("/shared/data/ref").is_some(), "{shared}");
    let lead = |term: String| json!({"kind": "related", "tool": "localSearch", "input": {"queries": [{"path": "src", "matchString": [term]}]}});
    let empty = |index: usize| {
        json!({"index": index, "status": "empty", "data": {
            "files": [], "hints": [format!("No match for {}.", token())], "related": [lead(token())]}})
    };
    let batch = hoisted_after_the_gate(
        ToolId::LocalSearch,
        json!({"results": [empty(0), empty(1)]}),
    );
    assert!(!leaks(&batch), "{batch}");
    assert_eq!(batch["hints"].as_array().map(Vec::len), Some(1), "{batch}");
    assert_eq!(
        batch["related"].as_array().map(Vec::len),
        Some(1),
        "{batch}"
    );
}

/// F3: a value equal to a redacted sibling is omitted only as the sealed
/// sibling reads, so the reader recovers the redacted text, not the token.
#[test]
fn an_omitted_value_reads_back_as_the_gate_sealed_it() {
    let symbol = json!({"symbolName": token(), "kind": "function", "line": 3,
        "testIdentity": {"framework": "rust", "name": token(), "parameterized": false, "kind": "declaration"}});
    let stated = hoisted_after_the_gate(
        ToolId::AstSearch,
        json!({"results": [{"index": 0, "data": {"files": [{"path": "a.rs", "symbols": [symbol]}]}}]}),
    );
    assert!(!leaks(&stated), "{stated}");
    // The embedded plan also states the outline as strings (Q5), joined from
    // the sealed declaration after the omission.
    let line = &stated["results"][0]["data"]["files"][0]["symbols"][0];
    let (_, row) = line
        .as_str()
        .and_then(crate::response::symbol_outline::outline_row)
        .unwrap_or_else(|| panic!("an outline line: {stated}"));
    assert_eq!(
        row["testIdentity"],
        json!({"framework": "rust"}),
        "{stated}"
    );
}

/// F2 runs in the tools, before the gate: a token in a grouped name is
/// judged in the text the response carries; so is a token in a declaration
/// whose name the gate reads whole as a string.
#[test]
fn groups_and_outline_strings_are_judged_as_the_text_they_carry() {
    let rows = (0..3)
        .map(|n| json!({"path": format!("packages/deep/dir/{}-{n}.txt", token())}))
        .collect::<Vec<_>>();
    let grouped =
        crate::response::groups::group_rows(&rows, crate::response::groups::Order::Merged)
            .expect("shorter grouped");
    let sealed = Sanitized::sealed(json!({"results": [{"index": 0, "data": {"files": grouped}}]}));
    assert!(!leaks(&sealed), "{}", *sealed);
    let outline = crate::response::symbol_outline::outline_lines(&[json!({
        "symbolName": token(), "kind": "function", "line": 3, "endLine": 9,
        "members": [{"symbolName": format!("inner_{}", token()), "kind": "function", "line": 4}],
    })]);
    let sealed =
        Sanitized::sealed(json!({"results": [{"index": 0, "data": {"symbols": outline}}]}));
    assert!(!leaks(&sealed), "{}", *sealed);
}

/// Q5 (fix-lsp's finding): a detector that reads a value only where it starts
/// its text (`PrivateKey = <key>`) hides it in a `symbolName` leaf; the
/// stage joins outline strings from the sealed leaves, so the string keeps
/// the redaction. astSearch symbols, lspSearch documentSymbols and
/// localSearch declarations, each formed by the stage.
#[test]
fn outline_strings_are_joined_from_the_sealed_declarations() {
    const WG: &str = "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=";
    let declaration = || json!({"symbolName": format!("PrivateKey = {WG}"), "kind": "variable", "line": 1, "endLine": 1});
    for (list, data) in [
        (
            "/files/*/symbols",
            json!({"files": [{"path": "a.conf", "symbols": [declaration()]}]}),
        ),
        ("/symbols", json!({"symbols": [declaration()]})),
        (
            "/files/*/declarations",
            json!({"files": [{"path": "a.conf", "declarations": [declaration()],
            "matches": [{"line": 1, "value": "x", "enclosing": 0}]}]}),
        ),
    ] {
        let plan = super::plan(Vec::new(), Vec::new(), Batch::default());
        let plan = Plan {
            outline: vec![Pattern::parse(list)],
            ..plan
        };
        let mut sealed = Sanitized::sealed(json!({"results": [{"index": 0, "data": data}]}));
        assert!(
            !sealed.to_string().contains(WG),
            "{list}: the gate hides the leaf"
        );
        sealed.edit(|structured| *structured = super::hoisted(&plan, structured, &super::optional));
        let stated = sealed.into_value();
        assert!(!stated.to_string().contains(WG), "{list}: {stated}");
        let lines = Pattern::parse(list).matches(&stated["results"][0]["data"]);
        let line = stated["results"][0]["data"]
            .pointer(&format!("{}/0", lines[0].pointer()))
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("{list}: an outline string: {stated}"));
        assert!(line.starts_with("1\tvariable "), "{list}: {line}");
    }
}
