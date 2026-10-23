//! Exact repairs: the one call a row spelled another way, a relocated brief, spans and the last reachable page.

use super::assert_executable;
use crate::support::{Run as _, Workspace};
use serde_json::{Value, json};

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
            json!({"ecosystem":"npm","keywords":["react"],"page":1001,"pageSize":50,
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
