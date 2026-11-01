use super::*;
use crate::execution::cancel::NeverCancel;
use crate::security::ContentSecurity;
use crate::tools::local_search::execute_local_search;
use octocode_contract_types::LocalSearchQuery;

/// The lexical lead is a call as given: each of its rows runs through
/// localSearch and finds the whole word, not the word's `\b` spelling.
fn run_rows(lead: &Value, root: &std::path::Path) -> Vec<usize> {
    let policy = crate::tools::test_support::workspace_policy(root);
    lead["input"]["queries"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| {
            let query: LocalSearchQuery =
                serde_json::from_value(row.clone()).expect("lead row is a localSearch query");
            execute_local_search(
                &query,
                &policy,
                &ContentSecurity::new(),
                &NeverCancel,
                None,
                None,
            )
            .expect("lead runs")
            .files
            .iter()
            .map(|file| file.matches.as_ref().map_or(0, Vec::len))
            .sum()
        })
        .collect()
}

#[test]
fn ignored_recovery_deduplicates_queries_and_reserves_room_for_unfinished_scope() {
    let scope = super::super::scope::Scope::new("/r".into(), vec!["*.ts".into()]);
    let files = (0..4).map(|i| format!("/r/f{i}.js")).collect::<Vec<_>>();
    let mut repeated = files.clone();
    repeated.extend(files.clone());
    let lead = ignored_text_lead("x", &scope, &repeated, true);
    let rows = lead["input"]["queries"].as_array().expect("queries");
    assert_eq!(rows.len(), LISTED_TEXT_FILES);
    for (row, path) in rows.iter().zip(&files) {
        assert_eq!(row["path"], *path);
        assert!(row.get("include").is_none());
        assert_eq!(row["includeIgnored"], true);
    }
    assert_eq!(rows[4]["path"], "/r");
    assert_eq!(rows[4]["include"], json!(["*.ts"]));
    assert_eq!(rows[4]["includeIgnored"], true);

    let scope = super::super::scope::Scope::new("/r".into(), Vec::new());
    let lead = ignored_text_lead("x", &scope, &["/r".into(), "/r".into()], true);
    assert_eq!(
        lead["input"]["queries"].as_array().expect("queries").len(),
        1
    );
}

#[test]
fn ignored_recovery_overflow_keeps_out_of_family_and_nested_paths_searchable() {
    let root = tempfile::tempdir().expect("root");
    std::fs::create_dir(root.path().join(".git")).expect("repository");
    std::fs::write(root.path().join(".gitignore"), "*.js\n").expect("ignore rules");
    let mut files = Vec::new();
    for relative in ["a.js", "b.js", ".hidden/c.js", "dist/d.js"] {
        let path = root.path().join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(&path, "needle();\n").expect("source");
        files.push(path.to_string_lossy().into_owned());
    }
    let nested = root.path().join("repos/child");
    std::fs::create_dir_all(nested.join(".git")).expect("nested repository");
    std::fs::write(nested.join("e.js"), "needle();\n").expect("nested source");
    files.push(nested.to_string_lossy().into_owned());
    let scope = super::super::scope::Scope::new(
        root.path().to_string_lossy().into_owned(),
        vec!["*.ts".into()],
    );
    let exact = ignored_text_lead("needle", &scope, &files, false);
    assert_eq!(
        exact["input"]["queries"].as_array().expect("queries").len(),
        5
    );
    assert_eq!(run_rows(&exact, root.path()), vec![1; 5]);

    let unfinished = ignored_text_lead("needle", &scope, &files, true);
    let rows = unfinished["input"]["queries"].as_array().expect("queries");
    assert_eq!(rows.len(), 2);
    assert!(rows[0].get("include").is_none());
    assert_eq!(rows[0]["defaultExcludes"], false);
    assert_eq!(rows[0]["includeHidden"], true);
    assert_eq!(rows[1]["include"], json!(["*.ts"]));
    assert_eq!(run_rows(&unfinished, root.path()), vec![5, 0]);

    let sixth = root.path().join("f.js");
    std::fs::write(&sixth, "needle();\n").expect("sixth source");
    files.push(sixth.to_string_lossy().into_owned());
    let complete = ignored_text_lead("needle", &scope, &files, false);
    assert_eq!(
        complete["input"]["queries"]
            .as_array()
            .expect("queries")
            .len(),
        1
    );
    assert_eq!(run_rows(&complete, root.path()), vec![6]);
}

#[test]
fn the_text_lead_finds_the_word_in_a_scope_and_in_listed_files() {
    let root = tempfile::tempdir().expect("root");
    let file = root.path().join("a.py");
    std::fs::write(&file, "get(1)\ngetter = 2\nx.get\n").expect("source");
    let scope = super::super::scope::Scope::new(
        root.path().to_string_lossy().into_owned(),
        vec!["*.py".into()],
    );
    let wide = text_search_lead("get", &scope.root, &scope.include, None).build();
    assert_eq!(run_rows(&wide, root.path()), vec![2], "{wide}");
    let files = vec![file.to_string_lossy().into_owned()];
    let listed = text_search_lead("get", &scope.root, &scope.include, Some(&files)).build();
    assert_eq!(run_rows(&listed, root.path()), vec![2], "{listed}");
    // A name whose edge is not a word character has no `\b` there.
    std::fs::write(&file, "$store = 1\n$stored = 2\n").expect("source");
    let dollar = text_search_lead("$store", &scope.root, &scope.include, None).build();
    assert_eq!(run_rows(&dollar, root.path()), vec![1], "{dollar}");
}

/// An unresolved anchor's read lead runs only when it finds the name: a
/// name the file holds only in another case is named at its nearest line
/// and read case-insensitively; a name the file lacks gets no read.
#[test]
fn anchor_read_lead_only_when_it_finds_the_name() {
    let query: LspSearchQuery = serde_json::from_value(json!({
        "operation": "definition", "path": "/w/lib.rs", "symbolName": "Runtime", "line": 1
    }))
    .expect("query");
    let message = "Could not find symbol 'Runtime' at or near line 1";
    let reads = |row: &Value| {
        row["related"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|call| call["tool"] == "localFetch")
            .map(|call| call["input"]["queries"][0].clone())
            .collect::<Vec<_>>()
    };
    let workspace_search = |row: &Value| {
        row["related"].as_array().is_some_and(|calls| {
            calls.iter().any(|call| {
                call["tool"] == "lspSearch"
                    && call["input"]["queries"][0]["operation"] == "workspaceSymbol"
            })
        })
    };
    let run = |source: &str| {
        let mut row = failure(&query, "", ErrorCode::AnchorUnresolved, message, true);
        anchor_recovery(&mut row, &query, Some(source));
        row
    };

    let row = run("// x\nstruct Runtime;\n");
    assert_eq!(reads(&row).len(), 1, "{row}");
    assert_eq!(reads(&row)[0]["caseMode"], "sensitive", "{row}");
    assert_eq!(row["error"], message);
    assert!(workspace_search(&row), "{row}");

    let row = run("mod a;\nfn runtime() {}\nfn b() {}\nlet RUNTIME = 1;\n");
    assert_eq!(
        row["error"],
        format!("{message}; the file has 'Runtime' only in another case, nearest at line 2")
    );
    assert_eq!(reads(&row).len(), 1, "{row}");
    assert_eq!(reads(&row)[0]["caseMode"], "insensitive", "{row}");
    assert!(workspace_search(&row), "{row}");

    let row = run("fn other() {}\n");
    assert!(reads(&row).is_empty(), "{row}");
    assert_eq!(row["error"], message);
    assert!(workspace_search(&row), "{row}");
}
