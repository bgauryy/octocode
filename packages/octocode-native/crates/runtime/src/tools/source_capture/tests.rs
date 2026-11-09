use super::*;

mod glob_resources;
mod key;

/// A selection over one capture's memory says how to judge it in parts.
#[test]
fn a_too_large_selection_says_how_to_judge_it_in_parts() {
    let error = too_large(1_000, "The selected source", IN_PARTS_HINT);
    assert_eq!(error.code, ErrorCode::ClassificationContextTooLarge);
    assert_eq!(error.hints, [IN_PARTS_HINT]);
    assert!(IN_PARTS_HINT.contains("operation:\"sections\""));
    assert!(error.message.starts_with("The selected source exceeds"));
}

fn one_char_pages(count: usize) -> impl FnMut(&Value) -> Result<page::Page, ClassificationError> {
    move |row: &Value| {
        let at = row["at"].as_u64().unwrap_or(0);
        Ok(page::Page {
            content: "x".to_owned(),
            pin: Some("pin".to_owned()),
            next: (at + 1 < count as u64).then(|| json!({ "at": at + 1 })),
        })
    }
}

/// A selection takes as many reader pages as it has: only the state limit
/// ends it, never a page count.
#[test]
fn a_selection_of_many_small_pages_is_captured_whole() {
    let (text, pin) =
        collect_pages(json!({"at": 0}), one_char_pages(500), 1_000, false).expect("captured");
    assert_eq!(text.len(), 500);
    assert_eq!(pin.as_deref(), Some("pin"));
    let error = collect_pages(json!({"at": 0}), one_char_pages(500), 499, false)
        .expect_err("over the state limit");
    assert_eq!(error.code, ErrorCode::ClassificationContextTooLarge);
}

/// A reader that offers a page it already served fails as a defect rather
/// than looping or claiming the selection is too large.
#[test]
fn a_reader_that_repeats_a_page_fails_without_looping() {
    let repeat = |_: &Value| {
        Ok(page::Page {
            content: "x".to_owned(),
            pin: None,
            next: Some(json!({"at": 0})),
        })
    };
    let error =
        collect_pages(json!({"at": 0}), repeat, 1_000_000, false).expect_err("repeated page");
    assert_eq!(error.code, ErrorCode::ClassificationContextFailed);
}

/// The GitHub tools that can send a row to a classifier, each passing the name
/// rule: a read by `withheld_github_source`, a provider page by its row units
/// (`selection::row_units`, which withhold credential-named paths). A GitHub
/// tool that gains `classification` has to pass one of them.
#[test]
fn only_the_covered_github_tools_take_classification() {
    let taking = ToolId::ALL
        .iter()
        .copied()
        .filter(|tool| {
            tool.is_github() && crate::contracts::query_declares(*tool, "classification")
        })
        .collect::<Vec<_>>();
    let covered = ToolId::ALL
        .iter()
        .copied()
        .filter(|tool| {
            tool.is_github() && (tool.selects_provider_page() || *tool == ToolId::GhGetFileContent)
        })
        .collect::<Vec<_>>();
    assert_eq!(taking, covered);
}

#[test]
fn a_withheld_github_source_is_refused_and_names_only_a_route_the_policy_opens() {
    let policy = |allow: bool| {
        let root = tempfile::tempdir().expect("root");
        PathPolicy::new(crate::policy::path::PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            allow_credential_files: allow,
            ..Default::default()
        })
        .expect("policy")
    };
    let env = json!({"owner":"o","repo":"r","path":".env"});
    let ssh = json!({"owner":"o","repo":"r","path":"a/.ssh/config","ref":"main"});
    for allow in [false, true] {
        let reads = policy(allow);
        // A sensitive directory opens to no read: no lead, and the hint says so.
        let error = withheld_github_source(&ssh, &reads).expect("withheld");
        assert_eq!(error.code, ErrorCode::PathPolicyDenied);
        assert!(error.leads.is_empty(), "{:?}", error.leads);
        assert!(
            error.hints.join(" ").contains("No read returns it"),
            "{:?}",
            error.hints
        );
        // A credential name: classification refuses it under both settings;
        // the plain read is the route only where the user's policy opens it.
        let error = withheld_github_source(&env, &reads).expect("withheld");
        assert_eq!(error.code, ErrorCode::PathPolicyDenied);
        let hints = error.hints.join(" ");
        if allow {
            assert_eq!(error.leads.len(), 1, "{:?}", error.leads);
            assert_eq!(error.leads[0].1["tool"], "ghGetFileContent");
            assert!(hints.contains("redacted"), "{hints}");
        } else {
            assert!(error.leads.is_empty(), "{:?}", error.leads);
            assert!(hints.contains("security.credentialFiles: allow"), "{hints}");
        }
        let normal = json!({"owner":"o","repo":"r","path":"src/a.rs"});
        assert!(withheld_github_source(&normal, &reads).is_none());
    }
}

/// A fresh read reruns the capture in any later runtime: a GitHub read at the
/// commit the capture resolved, whatever ref was asked, and a local read by
/// path, never with a snapshot this runtime ends with (CLS-C9).
#[test]
fn a_fresh_read_keeps_the_captured_commit_and_no_local_snapshot() {
    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
    let remote = json!({"owner":"octo","repo":"demo","path":"a.rs","ref":"main"});
    let read = fresh_read(ToolId::GhGetFileContent, &remote, Some(SHA));
    assert_eq!(read["kind"], "related", "{read:#}");
    assert_eq!(read["input"]["queries"][0]["ref"], SHA, "{read:#}");
    let unpinned = fresh_read(ToolId::GhGetFileContent, &remote, None);
    assert_eq!(
        unpinned["input"]["queries"][0]["ref"], "main",
        "{unpinned:#}"
    );
    let local = fresh_read(ToolId::LocalFetch, &json!({"path":"a.rs"}), Some("digest"));
    let row = &local["input"]["queries"][0];
    assert_eq!(row["path"], "a.rs", "{local:#}");
    assert!(row.get("snapshot").is_none(), "{local:#}");
}
