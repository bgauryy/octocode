//! A hit window of a line past `LONG_LINE_BYTES` that names a Kubernetes Secret
//! says when its walk outgrew the memory budget and every value is hidden.
use super::*;
use octocode_engine::security::{LONG_LINE_BYTES, set_walk_budget};

/// Holds the walk budget of the files under `dir` at `budget` until dropped.
struct Budget(std::path::PathBuf);

impl Budget {
    fn hold(dir: &std::path::Path, budget: Option<usize>) -> Self {
        set_walk_budget(dir, budget);
        Self(dir.to_owned())
    }
}

impl Drop for Budget {
    fn drop(&mut self) {
        set_walk_budget(&self.0, None);
    }
}

/// One line of Secret objects longer than a whole-file read, each value fake,
/// ending in the one object the search hits.
fn secret_line() -> String {
    let mut line = String::from(r#"{"items":["#);
    let mut index = 0;
    while line.len() < LONG_LINE_BYTES {
        line.push_str(&format!(
            r#"{{"data":{{"k":"v{index:06}"}},"kind":"Secret"}},"#
        ));
        index += 1;
    }
    line.push_str("{\"kind\":\"ConfigMap\"}]}\n");
    line
}

fn search_line(budget: Option<usize>) -> serde_json::Value {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir_all(root.path().join("src")).expect("dir");
    fs::write(root.path().join("src/objects.json"), secret_line()).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": "src", "matchString": ["ConfigMap"]}),
        None,
    );
    let dir = root.path().join("src").canonicalize().expect("dir");
    let _budget = Budget::hold(&dir, budget);
    search_in(&request, &policy, &security, None)
        .expect("search")
        .to_row()
        .map(|row| {
            assert_eq!(row["matchCount"], 1, "the line is searched: {row}");
            row
        })
        .expect("row")
}

fn reasons(row: &serde_json::Value) -> Vec<String> {
    row["partialReasons"]
        .as_array()
        .map(|all| {
            all.iter()
                .filter_map(|reason| reason.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_walk_over_its_budget_marks_the_row_with_the_every_value_reason() {
    let row = search_line(Some(1));
    assert_eq!(row["isPartial"], true, "{row}");
    assert!(
        reasons(&row).contains(&"securityLongTextEveryValue".to_owned()),
        "{row}"
    );
}

#[test]
fn a_walk_within_its_budget_leaves_the_row_whole() {
    let row = search_line(None);
    assert!(
        !reasons(&row).contains(&"securityLongTextEveryValue".to_owned()),
        "{row}"
    );
    assert!(row.get("isPartial").is_none(), "{row}");
}
