//! A rejected call's `repair` hint: the corrected call when every issue has
//! one exact fix, on the whole-call envelope and on an isolated batch row.

use crate::support::{self, Run as _};
use octocode_contract_types::ErrorCode;

use octocode_native::contracts;
use serde_json::{Value, json};
use support::Workspace;

const COMMIT: &str = "commit";

fn commit_row(sections: Value) -> Value {
    json!({
        "owner": "o", "repo": "r", "operation": COMMIT, "ref": "main",
        "sections": sections,
        "mainGoal": "Read the commit.", "reasoning": "Exercise repair leads."
    })
}

/// The repair a lead carries is a call its own tool admits unchanged.
fn assert_executable(tool: &str, repair: &Value) {
    assert_eq!(repair["tool"], tool, "{repair}");
    let admitted = contracts::prepare_many_and_validate(tool, repair["query"].clone());
    assert!(admitted.is_ok(), "lead rejected: {admitted:?}: {repair}");
}

#[tokio::test]
async fn an_enum_value_the_operation_lacks_leads_to_the_call_without_it() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let input = json!({"queries":[commit_row(json!(["files", "comments"]))]});
    let error = runtime
        .run("enum".into(), "ghGetHistoryItem".into(), input)
        .await
        .expect_err("comments is a pull-request section");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    let payload = error.payload.as_deref().expect("payload");
    let repair = &crate::support::checks::hint(payload, "repair");
    assert_executable("ghGetHistoryItem", repair);
    assert_eq!(
        repair["query"],
        json!({"queries":[commit_row(json!(["files"]))]}),
        "{payload}"
    );

    // A text-only MCP client reads the same lead in the error line.
    let mcp = runtime
        .run_mcp(
            "enum-mcp".into(),
            "ghGetHistoryItem".into(),
            json!({"queries":[commit_row(json!(["comments"]))]}),
        )
        .await
        .expect("MCP reports the rejection in-band");
    let text = mcp["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("; repair: ghGetHistoryItem {\"queries\":["),
        "{text}"
    );
    // Dropping the only value drops the field: the default sections.
    let query =
        &crate::support::checks::hint(&mcp["structuredContent"], "repair")["query"]["queries"][0];
    assert!(query.get("sections").is_none(), "{query}");
    runtime.close().await;
}

/// clasifyContext judges hit snippets: a path-list or count view is refused
/// at admission, before any scan, and the repair drops only the view.
#[tokio::test]
async fn a_selection_with_a_list_view_leads_to_the_snippet_view() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let row = json!({
        "path": ".", "matchString": "x",
        "clasifyContext": {"classifiers": ["Relevant?"]},
        "mainGoal": "Find x.", "reasoning": "Exercise repair leads."
    });
    let mut listed = row.clone();
    listed["resultView"] = json!("files");
    let error = runtime
        .run(
            "view".into(),
            "localSearch".into(),
            json!({"queries":[listed]}),
        )
        .await
        .expect_err("a files view has no snippets to classify");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    let payload = error.payload.as_deref().expect("payload");
    let repair = &crate::support::checks::hint(payload, "repair");
    assert_executable("localSearch", repair);
    assert_eq!(repair["query"], json!({"queries":[row]}), "{payload}");
    runtime.close().await;
}

#[tokio::test]
async fn a_field_the_operation_ignores_leads_to_the_call_without_it() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let input = json!({"queries":[{
        "operation": "symbols", "path": ".", "pattern": "eval($X)",
        "mainGoal": "List symbols.", "reasoning": "Exercise repair leads."
    }]});
    let error = runtime
        .run("sibling".into(), "astSearch".into(), input)
        .await
        .expect_err("pattern belongs to another operation");
    let payload = error.payload.as_deref().expect("payload");
    let repair = &crate::support::checks::hint(payload, "repair");
    assert_executable("astSearch", repair);
    assert!(
        repair["query"]["queries"][0].get("pattern").is_none(),
        "{repair}"
    );
    assert_eq!(repair["query"]["queries"][0]["operation"], "symbols");
    runtime.close().await;
}

#[tokio::test]
async fn a_fix_only_the_caller_knows_carries_no_lead() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_BETA", "1".into())]);
    // A missing field, an enum value no rule maps to one listed value (its
    // default would widen the call), and a near-miss field name each need
    // the caller; the message names the fix.
    for (tool, row) in [
        (
            "lspSearch",
            json!({"operation":"definition","path":"a.ts","mainGoal":"g","reasoning":"r"}),
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"x","caseMode":"ignore","mainGoal":"g","reasoning":"r"}),
        ),
        (
            "localFetch",
            json!({"path":"README.md","minify":"none","mainGoal":"g","reasoning":"r"}),
        ),
        (
            "localSearch",
            json!({"path":".","matchstring":"x","mainGoal":"g","reasoning":"r"}),
        ),
    ] {
        let error = runtime
            .run("none".into(), tool.into(), json!({"queries":[row]}))
            .await
            .expect_err("invalid");
        let payload = error.payload.as_deref().expect("payload");
        assert!(payload.get("hints").is_none(), "{tool}: {payload}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn dropping_a_constraint_the_caller_set_is_a_named_low_confidence_repair() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let input = json!({"queries":[{
        "path":".","matchString":"x","glob":"*.ts","mainGoal":"g","reasoning":"r"
    }]});
    let error = runtime
        .run("lossy".into(), "localSearch".into(), input)
        .await
        .expect_err("glob is not a localSearch field");
    let payload = error.payload.as_deref().expect("payload");
    let repair = &crate::support::checks::hint(payload, "repair");
    assert_executable("localSearch", repair);
    assert_eq!(repair["confidence"], "low", "{payload}");
    assert!(
        crate::support::checks::hint(payload, "text")[0]
            .as_str()
            .is_some_and(|note| note.contains("queries[0].glob")),
        "{payload}"
    );
    runtime.close().await;
}

/// The one call each rejected row means, exact: a near-miss enum value
/// (named in the error too), a `true` field an enum value names, a field
/// of the form whose omitted selector is one literal, a line-range typo, a
/// number for a text field, a joined `owner/repo`, a `.git` suffix, and a
/// GitHub web URL whose ref is a commit SHA.
#[tokio::test]
async fn exact_repairs_read_what_the_row_spelled_another_way() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let sha = "0123456789abcdef0123456789abcdef01234567";
    for (tool, row, fixed, message) in [
        (
            "structureSearch",
            json!({"operation":"files","path":".","entryType":"file"}),
            json!({"operation":"files","path":".","entryType":"f"}),
            Some("did you mean 'f'?"),
        ),
        (
            "lspSearch",
            json!({"operation":"refs","path":"src/a.ts","symbolName":"a","line":1}),
            json!({"operation":"references","path":"src/a.ts","symbolName":"a","line":1}),
            Some("did you mean 'references'?"),
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"a.b","regex":"regex"}),
            json!({"path":".","matchString":"a.b","regex":"rust"}),
            None,
        ),
        (
            "ghSearchHistory",
            json!({"operation":"pr","owner":"o","repo":"r"}),
            json!({"operation":"pullRequest","owner":"o","repo":"r"}),
            Some("did you mean 'pullRequest'?"),
        ),
        (
            "ghSearchHistory",
            json!({"operation":"pullRequest","owner":"o","repo":"r","merged":true}),
            json!({"operation":"pullRequest","owner":"o","repo":"r","state":"merged"}),
            Some("did you mean 'state:\"merged\"'?"),
        ),
        (
            "structureSearch",
            json!({"path":".","nameRegex":"x"}),
            json!({"path":".","nameRegex":"x","operation":"files"}),
            Some("set it, or remove 'nameRegex'"),
        ),
        (
            "localFetch",
            json!({"path":"README.md","ranges":["3:6"]}),
            json!({"path":"README.md","ranges":["3-6"]}),
            Some("did you mean \"3-6\"?"),
        ),
        (
            "artifactSearch",
            json!({"ecosystem":"npm","packageName":"react","version":18}),
            json!({"ecosystem":"npm","packageName":"react","version":"18"}),
            None,
        ),
        (
            "ghSearchCode",
            json!({"owner":"facebook/react","keywords":["useState"]}),
            json!({"owner":"facebook","repo":"react","keywords":["useState"]}),
            Some("send owner:\"facebook\", repo:\"react\""),
        ),
        (
            "ghSearchRepo",
            json!({"repo":"sindresorhus/is"}),
            json!({"owner":"sindresorhus","repo":"is"}),
            None,
        ),
        (
            "ghStructure",
            json!({"owner":"sindresorhus","repo":"is.git"}),
            json!({"owner":"sindresorhus","repo":"is"}),
            Some("send repo:\"is\""),
        ),
        (
            "ghGetFileContent",
            json!({"path": format!("https://github.com/o/r/blob/{sha}/src/a.rs")}),
            json!({"path":"src/a.rs","owner":"o","repo":"r","ref":sha}),
            Some("is a repository web URL"),
        ),
        // A field named for a kind of value `ref` takes becomes `ref`.
        (
            "ghStructure",
            json!({"owner":"facebook","repo":"react","operation":"refs","branch":"main"}),
            json!({"owner":"facebook","repo":"react","operation":"refs","ref":"main"}),
            Some("did you mean 'ref'?"),
        ),
        (
            "ghGetFileContent",
            json!({"owner":"o","repo":"r","path":"a.rs","tag":"v1.0.0"}),
            json!({"owner":"o","repo":"r","path":"a.rs","ref":"v1.0.0"}),
            None,
        ),
        (
            "ghSearchCode",
            json!({"owner":"o","repo":"r","keywords":["x"],"branch":"dev"}),
            json!({"owner":"o","repo":"r","keywords":["x"],"ref":"dev"}),
            None,
        ),
        (
            "ghSearchHistory",
            json!({"operation":"commit","owner":"o","repo":"r","branch":"dev"}),
            json!({"operation":"commit","owner":"o","repo":"r","ref":"dev"}),
            None,
        ),
        (
            "ghGetHistoryItem",
            json!({"operation":"commit","owner":"o","repo":"r","sha":sha}),
            json!({"operation":"commit","owner":"o","repo":"r","ref":sha}),
            None,
        ),
        (
            "ghCloneRepo",
            json!({"owner":"o","repo":"r","branch":"dev"}),
            json!({"owner":"o","repo":"r","ref":"dev"}),
            None,
        ),
        // The default (1) is the minimum a depth of zero clamps to: exact.
        (
            "ghCloneRepo",
            json!({"owner":"sindresorhus","repo":"is","historyDepth":0}),
            json!({"owner":"sindresorhus","repo":"is","historyDepth":1}),
            None,
        ),
    ] {
        let error = runtime
            .run(tool.into(), tool.into(), json!({"queries":[row]}))
            .await
            .expect_err("invalid");
        let payload = error.payload.as_deref().expect("payload");
        let repair = &crate::support::checks::hint(payload, "repair");
        assert_executable(tool, repair);
        assert!(
            repair.get("confidence").is_none(),
            "{tool}: exact: {payload}"
        );
        assert_eq!(
            repair["query"],
            json!({"queries":[fixed]}),
            "{tool}: {payload}"
        );
        if let Some(message) = message {
            assert!(
                payload.to_string().contains(&message.replace('"', "\\\"")),
                "{tool}: {payload}"
            );
        }
    }
    runtime.close().await;
}

/// A ref read from a web URL may end inside the file path: the lead is
/// low confidence and names the guessed fields; a field of another form
/// the row's own selector excludes is dropped, named.
#[tokio::test]
async fn approximate_and_lossy_coordinate_repairs_are_named() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for (tool, row, fixed, named) in [
        (
            "ghGetFileContent",
            json!({"path":"https://github.com/o/r/blob/main/src/a.rs"}),
            json!({"path":"src/a.rs","owner":"o","repo":"r","ref":"main"}),
            "queries[0].path, queries[0].ref",
        ),
        (
            "ghSearchHistory",
            json!({"owner":"o","repo":"r","operation":"pullRequest","path":"src/a.rs"}),
            json!({"owner":"o","repo":"r","operation":"pullRequest"}),
            "queries[0].path",
        ),
    ] {
        let error = runtime
            .run(tool.into(), tool.into(), json!({"queries":[row]}))
            .await
            .expect_err("invalid");
        let payload = error.payload.as_deref().expect("payload");
        let repair = &crate::support::checks::hint(payload, "repair");
        assert_executable(tool, repair);
        assert_eq!(repair["confidence"], "low", "{tool}: {payload}");
        assert_eq!(
            repair["query"],
            json!({"queries":[fixed]}),
            "{tool}: {payload}"
        );
        assert!(
            crate::support::checks::hint(payload, "text")[0]
                .as_str()
                .is_some_and(|note| note.contains(named)),
            "{tool}: {payload}"
        );
    }
    runtime.close().await;
}

/// A minDepth past maxDepth and a URL package name are refused with the
/// fix named and no lead: only the caller knows the intended value.
#[tokio::test]
async fn rule_refusals_name_the_fix_without_a_lead() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for (tool, row, message) in [
        (
            "structureSearch",
            json!({"operation":"files","path":".","minDepth":3,"maxDepth":1}),
            "minDepth must be less than or equal to maxDepth",
        ),
        (
            "artifactSearch",
            json!({"ecosystem":"npm","packageName":"https://www.npmjs.com/package/react"}),
            "is a URL; send the name its manifest uses",
        ),
        // A value below the minimum of a field the row requires names what
        // to read: no bound is a safe guess, so the message names the range.
        (
            "ghGetHistoryItem",
            json!({"owner":"facebook","repo":"react","operation":"pullRequest","number":-1}),
            "outside the allowed range (1-",
        ),
        (
            "lspSearch",
            json!({"operation":"definition","path":"src/a.ts","symbolName":"a","line":0}),
            "outside the allowed range (1-",
        ),
    ] {
        let error = runtime
            .run(tool.into(), tool.into(), json!({"queries":[row]}))
            .await
            .expect_err("invalid");
        let payload = error.payload.as_deref().expect("payload");
        assert!(
            payload["error"]
                .as_str()
                .is_some_and(|error| error.contains(message)),
            "{payload}"
        );
        assert!(
            crate::support::checks::hint(payload, "repair").is_null(),
            "{payload}"
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn exact_repairs_relocate_a_root_brief_and_parse_a_numeric_string() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let input = json!({"mainGoal":"g","queries":[
        {"path":".","matchString":"x","reasoning":"r"},
        {"path":".","matchString":"y","contextLines":"3","mainGoal":"own","reasoning":"r"}
    ]});
    let error = runtime
        .run("exact".into(), "localSearch".into(), input)
        .await
        .expect_err("root mainGoal and a string number");
    let payload = error.payload.as_deref().expect("payload");
    let repair = &crate::support::checks::hint(payload, "repair");
    assert_executable("localSearch", repair);
    assert!(
        repair.get("confidence").is_none(),
        "exact repair is unqualified: {payload}"
    );
    let rows = &repair["query"]["queries"];
    assert_eq!(rows[0]["mainGoal"], "g");
    assert_eq!(rows[1]["mainGoal"], "own", "a row's own brief wins");
    assert_eq!(rows[1]["contextLines"], 3);
    assert!(repair["query"].get("mainGoal").is_none());
    runtime.close().await;
}

/// A whole-file read with a window control: the repair drops exactly the
/// controls, keeping the whole read.
#[tokio::test]
async fn a_full_read_with_window_controls_leads_to_the_whole_read() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "one\ntwo\n");
    let runtime = workspace.runtime(&[]);
    let input =
        json!({"queries":[{"path": file, "fullContent": true, "length": 100, "unit": "lines"}]});
    let error = runtime
        .run("full".into(), "localFetch".into(), input)
        .await
        .expect_err("fullContent takes no window");
    let payload = error.payload.as_deref().expect("payload");
    let repair = &crate::support::checks::hint(payload, "repair");
    assert_executable("localFetch", repair);
    assert!(
        repair.get("confidence").is_none(),
        "exact repair is unqualified: {payload}"
    );
    assert_eq!(
        repair["query"],
        json!({"queries":[{"path": file, "fullContent": true}]}),
        "{payload}"
    );
    runtime.close().await;
}

/// A row that is itself an envelope is refused, never unwrapped: the
/// repair lifts its rows and keeps its envelope fields beside them.
#[tokio::test]
async fn a_nested_envelope_leads_to_the_flat_call() {
    let workspace = Workspace::new();
    let file = workspace.write("src/a.txt", "one\n");
    let runtime = workspace.runtime(&[]);
    let input = json!({"queries":[{"responseLength": 3000, "queries":[{"path": file}]}]});
    let error = runtime
        .run("nested".into(), "localFetch".into(), input)
        .await
        .expect_err("nested envelope");
    let payload = error.payload.as_deref().expect("payload");
    assert!(!payload.to_string().contains("prepare_many"), "{payload}");
    let repair = &crate::support::checks::hint(payload, "repair");
    assert_executable("localFetch", repair);
    assert_eq!(
        repair["query"],
        json!({"queries":[{"path": file}], "responseLength": 3000}),
        "{payload}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn encoded_containers_are_rejected_and_offer_only_schema_valid_repairs() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for (tool, input, decoded) in [
        (
            "ghSearchCode",
            json!({"queries":[{"owner":"o","repo":"r","keywords":"[\"one\",\"two\"]"}]}),
            json!({"queries":[{"owner":"o","repo":"r","keywords":["one","two"]}]}),
        ),
        (
            "localFetch",
            json!({"queries":"[{\"path\":\"a.rs\",\"ranges\":[\"1-2\"]}]"}),
            json!({"queries":[{"path":"a.rs","ranges":["1-2"]}]}),
        ),
    ] {
        let error = runtime
            .run("encoded".into(), tool.into(), input.clone())
            .await
            .expect_err("strict admission rejects JSON text");
        assert_eq!(error.code, ErrorCode::InvalidInput);
        let repair = support::checks::hint(error.payload.as_deref().expect("payload"), "repair");
        assert_executable(tool, &repair);
        assert_eq!(repair["query"], decoded);
        assert!(
            contracts::prepare_many_and_validate(tool, input).is_err(),
            "repair never changes admission"
        );
    }
    for encoded in ["[\"unfinished", "{}", "[null]"] {
        let input = json!({"queries":[{"owner":"o","repo":"r","keywords":encoded}]});
        let error = runtime
            .run("invalid-encoded".into(), "ghSearchCode".into(), input)
            .await
            .expect_err("invalid input");
        assert!(
            support::checks::hint(error.payload.as_deref().expect("payload"), "repair").is_null()
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn scalar_array_items_offer_a_complete_repair_without_changing_admission() {
    let workspace = Workspace::new();
    let file = workspace.write("source.txt", "one\ntwo\n");
    let runtime = workspace.runtime(&[]);
    for (tool, input, expected) in [
        (
            "ghSearchCode",
            json!({"queries":[{"owner":"owner","repo":"repo","keywords":"a phrase","pageSize":"10"}]}),
            json!({"queries":[{"owner":"owner","repo":"repo","keywords":["a phrase"],"pageSize":10}]}),
        ),
        (
            "ghSearchHistory",
            json!({"queries":[{"operation":"issue","owner":"owner","repo":"repo","keywords":"exact phrase"}]}),
            json!({"queries":[{"operation":"issue","owner":"owner","repo":"repo","keywords":["exact phrase"]}]}),
        ),
        (
            "localFetch",
            json!({"queries":[{"path":file,"ranges":"1-2"}]}),
            json!({"queries":[{"path":file,"ranges":["1-2"]}]}),
        ),
    ] {
        let error = runtime
            .run("scalar-array".into(), tool.into(), input.clone())
            .await
            .expect_err("strict admission rejects scalar array fields");
        assert_eq!(error.code, ErrorCode::InvalidInput);
        let repair = support::checks::hint(error.payload.as_deref().expect("payload"), "repair");
        assert_executable(tool, &repair);
        assert_eq!(repair["query"], expected);
        assert!(contracts::prepare_many_and_validate(tool, input).is_err());
        if tool == "localFetch" {
            runtime
                .run("repaired".into(), tool.into(), repair["query"].clone())
                .await
                .expect("repair executes against the real local file");
        }
    }
    runtime.close().await;
}

/// A `startLine`/`endLine` pair becomes the exact `ranges` span, and an
/// artifactSearch page past the deepest page its search reaches repairs to
/// that page in one hop. (A field of another operation, even one that
/// resembles `page`, is dropped: `approximate_and_lossy_coordinate_repairs_are_named`.)
#[tokio::test]
async fn exact_repairs_spell_spans_and_reach_the_last_page() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for (tool, row, check) in [
        (
            "localFetch",
            json!({"path":"README.md","startLine":5,"endLine":9,"mainGoal":"g","reasoning":"r"}),
            ("ranges", json!(["5-9"])),
        ),
        (
            "artifactSearch",
            json!({"ecosystem":"npm","keywords":["react"],"page":1001,
                "mainGoal":"g","reasoning":"r"}),
            ("page", json!(101)),
        ),
    ] {
        let error = runtime
            .run(tool.into(), tool.into(), json!({"queries":[row]}))
            .await
            .expect_err("invalid");
        let payload = error.payload.as_deref().expect("payload");
        let repair = &crate::support::checks::hint(payload, "repair");
        assert_executable(tool, repair);
        assert_ne!(repair["confidence"], "low", "{tool}: {payload}");
        let fixed = &repair["query"]["queries"][0];
        assert_eq!(
            fixed.get(check.0).cloned().unwrap_or(Value::Null),
            check.1,
            "{tool}: {payload}"
        );
        assert!(fixed.get("startLine").is_none() && fixed.get("endLine").is_none());
    }
    runtime.close().await;
}

/// A field that fails its pattern names itself and the pattern, not the
/// usage prose of its description.
#[tokio::test]
async fn a_pattern_refusal_names_the_field_and_the_pattern() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let error = runtime
        .run(
            "ghSearchRepo".into(),
            "ghSearchRepo".into(),
            json!({"queries":[{"owner":"a b"}]}),
        )
        .await
        .expect_err("invalid");
    let payload = error.payload.as_deref().expect("payload");
    let message = payload["error"].as_str().expect("message");
    assert!(
        message.contains(r#""a b" is not a valid owner"#),
        "{message}"
    );
    assert!(message.contains(r"^[\w.][\w.-]*$"), "{message}");
    assert!(!message.contains("newest push first"), "{message}");
    runtime.close().await;
}

/// An unknown field named for a kind of value that `ref` takes does not
/// overwrite a `ref` the row sets: the lead drops it, named.
#[tokio::test]
async fn a_kind_field_never_overwrites_the_ref_the_row_sets() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let error = runtime
        .run(
            "ghStructure".into(),
            "ghStructure".into(),
            json!({"queries":[{"owner":"o","repo":"r","operation":"refs","ref":"v1","branch":"main"}]}),
        )
        .await
        .expect_err("invalid");
    let payload = error.payload.as_deref().expect("payload");
    let repair = &crate::support::checks::hint(payload, "repair");
    assert_eq!(repair["confidence"], "low", "{payload}");
    assert_eq!(
        repair["query"],
        json!({"queries":[{"owner":"o","repo":"r","operation":"refs","ref":"v1"}]}),
        "{payload}"
    );
    runtime.close().await;
}
