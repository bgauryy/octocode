//! Code sites outside an answer inside files it covers: candidates per file
//! and line, by what the answer lists of each file; a gap only once the
//! server's identity check says a site names the anchor.
use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

const SOURCE: &str = "import { scene } from './scene';\nscene.replaceAllElements([]);\nscene.replaceAllElements([1]);\nconst f = scene.replaceAllElements;\n// replaceAllElements in a comment\nfunction g() { scene.replaceAllElements([2]); }\n";

fn answered(root: &Path, calls: bool, listed: &[u32]) -> (Scope, String) {
    let file = root.join("a.ts");
    write(&file, SOURCE);
    let path = file.to_string_lossy().into_owned();
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
    scope.answer([path.clone()]);
    scope.answer_lines(calls, listed.iter().map(|line| (path.as_str(), *line)));
    (scope, path)
}

/// The calls a `callers` answer does not list are candidates; a reference
/// that is no call, a comment and the lines it lists are none.
#[test]
fn a_callers_answer_misses_the_calls_it_leaves_out() {
    let (_dir, root) = fixture();
    let (scope, path) = answered(&root, true, &[1]);
    assert_eq!(
        *scope.answer_gaps("replaceAllElements"),
        [answered::Gap {
            file: path,
            left_out: Vec::new(),
            unverified: vec![2, 5],
        }]
    );
}

/// Every code use a `references` answer does not list is a candidate.
#[test]
fn a_references_answer_misses_every_use_it_leaves_out() {
    let (_dir, root) = fixture();
    let (scope, path) = answered(&root, false, &[1, 2]);
    assert_eq!(
        *scope.answer_gaps("replaceAllElements"),
        [answered::Gap {
            file: path,
            left_out: Vec::new(),
            unverified: vec![3, 5],
        }]
    );
    let (scope, _) = answered(&root, false, &[1, 2, 3, 5]);
    assert!(scope.answer_gaps("replaceAllElements").is_empty());
}

fn callers_row(scope: &Scope, path: &str, root: &Path) -> serde_json::Value {
    let query: crate::lsp_search::LspSearchQuery = serde_json::from_value(json!({
        "path": path, "operation": "callers", "symbolName": "replaceAllElements", "line": 2
    }))
    .expect("query");
    let mut row = json_fixture(r#"{"operation": "callers", "files": []}"#);
    crate::lsp_search::failure::flag_answer_gaps(&mut row, &query, scope, &workspace_policy(root));
    row
}

/// A spelling is no identity: a site no check placed is named as unverified
/// and leads to a text search of the file, and the row is not partial.
#[test]
fn an_unplaced_site_is_named_unverified_and_leaves_the_row_complete() {
    let (_dir, root) = fixture();
    let (scope, path) = answered(&root, true, &[1, 2]);
    let row = callers_row(&scope, &path, &root);
    assert!(row.get("isPartial").is_none(), "{row}");
    assert!(row.get("partialReasons").is_none(), "{row}");
    let warnings = row["warnings"].to_string();
    assert!(
        warnings.contains("a.ts:6") && warnings.contains("unverified"),
        "{row}"
    );
    assert!(!warnings.contains("leaves out"), "{row}");
    let lead =
        octocode_runtime_base::test_support::checks::related_call(&row, "localSearch", |_| true);
    assert_eq!(lead["input"]["queries"][0]["path"], "a.ts", "{row}");
}

/// A site the identity check resolved to the anchor is a use the answer
/// left out: the row is partial, names the site, and leads to its file. A
/// site resolved to another symbol is dropped, and a file left with no
/// site is no gap.
#[test]
fn a_site_that_names_the_anchor_flags_the_row_and_leads_to_its_file() {
    let (_dir, root) = fixture();
    let (scope, path) = answered(&root, true, &[1]);
    scope.answer_gaps("replaceAllElements");
    scope.settle_gaps(
        "replaceAllElements",
        vec![answered::Gap {
            file: path.clone(),
            left_out: vec![5],
            unverified: Vec::new(),
        }],
    );
    let row = callers_row(&scope, &path, &root);
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["serverUnanswered"]"#),
        "{row}"
    );
    assert!(
        row.to_string().contains(
            "leaves out 1 line where `replaceAllElements` names the anchor as a call, in files it answers: a.ts:6"
        ),
        "{row}"
    );
    assert!(!row["warnings"].to_string().contains("a.ts:3"), "{row}");
    let lead =
        octocode_runtime_base::test_support::checks::related_call(&row, "localSearch", |_| true);
    assert_eq!(lead["input"]["queries"][0]["path"], "a.ts", "{row}");

    let (scope, path) = answered(&root, true, &[1]);
    scope.settle_gaps(
        "replaceAllElements",
        vec![answered::Gap {
            file: path.clone(),
            left_out: Vec::new(),
            unverified: Vec::new(),
        }],
    );
    assert!(scope.answer_gaps("replaceAllElements").is_empty());
    let row = callers_row(&scope, &path, &root);
    assert_eq!(
        row,
        json_fixture(r#"{"operation": "callers", "files": []}"#)
    );
}

/// A warning states each file once per category, without losing any site or
/// confusing an unresolved spelling with a verified omitted call. Counts
/// are sites, not file groups; the lexical recovery still names both files.
#[test]
fn answer_gap_warnings_group_files_without_losing_sites_or_categories() {
    let (_dir, root) = fixture();
    let (scope, path) = answered(&root, true, &[1]);
    let other = root.join("nested/β.ts");
    write(&other, SOURCE);
    let other = other.to_string_lossy().into_owned();
    scope.answer([other.clone()]);
    scope.answer_gaps("replaceAllElements");
    scope.settle_gaps(
        "replaceAllElements",
        vec![
            answered::Gap {
                file: path.clone(),
                left_out: vec![2, 5],
                unverified: vec![3],
            },
            answered::Gap {
                file: other,
                left_out: vec![1],
                unverified: vec![2, 5],
            },
        ],
    );
    let row = callers_row(&scope, &path, &root);
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["serverUnanswered"]"#),
        "{row}"
    );
    let warnings = row["warnings"].as_array().expect("warnings");
    assert_eq!(
        warnings.len(),
        2,
        "one warning per evidence category: {row}"
    );
    for (warning, positions, category) in [
        (
            &warnings[0],
            ["a.ts:3,6", "nested/β.ts:2"],
            "leaves out 3 lines",
        ),
        (
            &warnings[1],
            ["a.ts:4", "nested/β.ts:3,6"],
            "3 lines in files",
        ),
    ] {
        let warning = warning.as_str().expect("warning text");
        assert!(warning.contains(category), "{warning}");
        for (file, positions) in ["a.ts", "nested/β.ts"].into_iter().zip(positions) {
            assert_eq!(warning.matches(file).count(), 1, "{warning}");
            assert!(warning.contains(positions), "{warning}");
        }
    }
    assert!(warnings[1].as_str().unwrap().contains("unverified"));
    assert!(!warnings[1].as_str().unwrap().contains("leaves out"));
    let lead =
        octocode_runtime_base::test_support::checks::related_call(&row, "localSearch", |_| true);
    let queries = lead["input"]["queries"]
        .as_array()
        .expect("recovery queries");
    assert_eq!(queries.len(), 2);
    assert_eq!(queries[0]["path"], "a.ts");
    assert_eq!(queries[1]["path"], "nested/β.ts");
}
