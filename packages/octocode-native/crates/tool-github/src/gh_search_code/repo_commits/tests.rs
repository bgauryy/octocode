use super::*;

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn row(repo: &str, path: &str, commit: Option<&str>) -> Value {
    let mut row = json!({"owner":"o","repo":repo,"path":path});
    if let Some(commit) = commit {
        row[COMMIT] = json!(commit);
    }
    row
}

/// The commit a reader finds for each row of `data`, by the reading rule:
/// its own, else its repository's entry in the table.
fn commits(data: &Value) -> Vec<(String, Option<String>)> {
    let table: Vec<(&str, &str)> = data[FIELD]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| {
            entry
                .as_str()
                .and_then(|entry| entry.split_once(' '))
                .expect("a \"repo sha\" entry")
        })
        .collect();
    data["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|row| {
            let repo = row["repo"].as_str().expect("repo");
            let own = row[COMMIT].as_str();
            let stated = table
                .iter()
                .find(|(name, _)| *name == repo)
                .map(|(_, commit)| *commit);
            assert!(
                own.is_none() || stated.is_none(),
                "one statement of a row's commit: {row}"
            );
            (
                format!("{repo}/{}", row["path"].as_str().expect("path")),
                own.or(stated).map(str::to_owned),
            )
        })
        .collect()
}

fn page(rows: Vec<Value>) -> (Value, Value) {
    let before = json!({"files": rows});
    let mut after = before.clone();
    state(&mut after);
    (before, after)
}

#[test]
fn rows_of_one_repository_and_commit_state_the_commit_once_and_keep_their_order() {
    let (before, after) = page(vec![
        row("a", "1", Some(A)),
        row("b", "2", Some(B)),
        row("a", "3", Some(A)),
        row("c", "4", Some(B)),
        row("a", "5", Some(A)),
    ]);
    assert_eq!(after[FIELD], json!([format!("a {A}")]), "{after}");
    // The table is stated before the rows that read it.
    let keys: Vec<&String> = after.as_object().expect("data").keys().collect();
    assert_eq!(keys, [FIELD, "files"], "{after}");
    // Every row still finds the commit it was read at, in the same order.
    assert_eq!(commits(&after), commits(&before));
    assert!(
        after.to_string().len() < before.to_string().len(),
        "{after}"
    );
    for row in after["files"].as_array().expect("files") {
        assert!(row["repo"].is_string(), "{row}");
        assert_eq!(row.get(COMMIT).is_none(), row["repo"] == "a", "{row}");
    }
}

#[test]
fn a_page_never_grows_and_never_loses_a_rows_commit() {
    let long = "r".repeat(90);
    for rows in [
        // One repository: `data` takes repo and commit.
        vec![row("a", "1", Some(A)), row("a", "2", Some(A))],
        // No repository repeats.
        vec![row("a", "1", Some(A)), row("b", "2", Some(B))],
        // A repository read at two commits.
        vec![
            row("a", "1", Some(A)),
            row("a", "2", Some(B)),
            row("b", "3", Some(B)),
        ],
        // A row without a commit.
        vec![
            row("a", "1", Some(A)),
            row("a", "2", None),
            row("b", "3", Some(B)),
        ],
        // A name longer than the commits its two rows repeat.
        vec![
            row(&long, "1", Some(A)),
            row(&long, "2", Some(A)),
            row("b", "3", Some(B)),
        ],
    ] {
        let (before, after) = page(rows);
        assert_eq!(after, before, "nothing to state once");
    }
    // Two owners: a repository name alone would not name the row's repository.
    let mut other = row("a", "3", Some(A));
    other["owner"] = json!("p");
    let (before, after) = page(vec![
        row("a", "1", Some(A)),
        row("a", "2", Some(A)),
        row("b", "4", Some(B)),
        other,
    ]);
    assert_eq!(after, before);
    for repeats in 2..6 {
        let mut rows = vec![row("z", "0", Some(B))];
        rows.extend((0..repeats).map(|at| row("a", &at.to_string(), Some(A))));
        rows.extend((0..repeats).map(|at| row(&long, &at.to_string(), Some(B))));
        let (before, after) = page(rows);
        assert!(after.to_string().len() <= before.to_string().len());
        assert_eq!(commits(&after), commits(&before));
    }
}

/// A selection hides rows after the page stated its table: stating again
/// reads the rows that are left, and no entry outlives its rows.
#[test]
fn stating_again_after_rows_left_restates_from_the_rows_shown() {
    let (before, mut after) = page(vec![
        row("a", "1", Some(A)),
        row("a", "2", Some(A)),
        row("b", "3", Some(B)),
        row("b", "4", Some(A)),
    ]);
    assert_eq!(after[FIELD], json!([format!("a {A}")]), "{after}");
    let mut again = after.clone();
    state(&mut again);
    assert_eq!(again, after, "stating twice changes nothing");
    // The selection hides one row of each repository.
    {
        let files = after["files"].as_array_mut().expect("files");
        files.remove(3);
        files.remove(1);
    }
    state(&mut after);
    assert!(after.get(FIELD).is_none(), "{after}");
    let left = commits(&after);
    let all = commits(&before);
    assert_eq!(left, [all[0].clone(), all[2].clone()]);
}
