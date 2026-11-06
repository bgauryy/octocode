//! Code sites a server leaves out inside files it answers: named per file
//! and line, by what the answer lists of each file.
use super::*;
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

/// A `callers` answer misses the calls it does not list; a reference that
/// is no call, a comment and the lines it lists are none.
#[test]
fn a_callers_answer_misses_the_calls_it_leaves_out() {
    let (_dir, root) = fixture();
    let (scope, path) = answered(&root, true, &[1]);
    assert_eq!(
        *scope.answer_gaps("replaceAllElements"),
        [answered::Gap {
            file: path,
            lines: vec![2, 5],
        }]
    );
}

/// A `references` answer misses every code use it leaves out.
#[test]
fn a_references_answer_misses_every_use_it_leaves_out() {
    let (_dir, root) = fixture();
    let (scope, path) = answered(&root, false, &[1, 2]);
    assert_eq!(
        *scope.answer_gaps("replaceAllElements"),
        [answered::Gap {
            file: path,
            lines: vec![3, 5],
        }]
    );
    let (scope, _) = answered(&root, false, &[1, 2, 3, 5]);
    assert!(scope.answer_gaps("replaceAllElements").is_empty());
}

/// The row is partial, names each missed site, and leads to a text search
/// of the file.
#[test]
fn a_missed_site_flags_the_row_and_leads_to_its_file() {
    let (_dir, root) = fixture();
    let (scope, path) = answered(&root, true, &[1, 2]);
    let query: crate::tools::lsp_search::LspSearchQuery = serde_json::from_value(json!({
        "path": path, "operation": "callers", "symbolName": "replaceAllElements", "line": 2
    }))
    .expect("query");
    let mut row = json!({"operation": "callers", "files": []});
    crate::tools::lsp_search::failure::flag_answer_gaps(
        &mut row,
        &query,
        &scope,
        &workspace_policy(&root),
    );
    assert_eq!(row["partialReasons"], json!(["serverUnanswered"]), "{row}");
    assert!(
        row.to_string().contains(
            "leaves out 1 code call(s) of `replaceAllElements` in files it answers: a.ts:6"
        ),
        "{row}"
    );
    let lead = crate::tools::test_support::checks::related_call(&row, "localSearch", |_| true);
    assert_eq!(lead["input"]["queries"][0]["path"], "a.ts", "{row}");
}
