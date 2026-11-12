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
fn the_text_lead_finds_the_word_in_a_scope_and_in_listed_files() {
    let root = tempfile::tempdir().expect("root");
    let file = root.path().join("a.py");
    std::fs::write(&file, "get(1)\ngetter = 2\nx.get\n").expect("source");
    let scope = super::super::scope::Scope::new(
        root.path().to_string_lossy().into_owned(),
        vec!["*.py".into()],
    );
    let wide = text_search_lead("get", &scope.root, &scope.include, None)
        .expect("lead")
        .build();
    assert_eq!(run_rows(&wide, root.path()), vec![2], "{wide}");
    let files = vec![file.to_string_lossy().into_owned()];
    let listed = text_search_lead("get", &scope.root, &scope.include, Some(&files))
        .expect("lead")
        .build();
    assert_eq!(run_rows(&listed, root.path()), vec![2], "{listed}");
    // A name whose edge is not a word character has no `\b` there.
    std::fs::write(&file, "$store = 1\n$stored = 2\n").expect("source");
    let dollar = text_search_lead("$store", &scope.root, &scope.include, None)
        .expect("lead")
        .build();
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
        let mut row = failure(&query, "", ErrorCode::NotFound, message, true);
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
