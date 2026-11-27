use octocode_contract_types::LocalSearchQuery;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::security::ContentSecurity;
use octocode_tool_local::local_search::execute_local_search;
use octocode_tool_lsp::lsp_search::failure::text_search_lead;
use serde_json::Value;

/// The lexical lead is a call as given: each of its rows runs through
/// localSearch and finds the whole word, not the word's `\b` spelling.
fn run_rows(lead: &Value, root: &std::path::Path) -> Vec<usize> {
    let policy = octocode_runtime_base::test_support::workspace_policy(root);
    lead["input"]["queries"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| {
            let query: LocalSearchQuery =
                serde_json::from_value(row.clone()).expect("lead row is a localSearch query");
            execute_local_search(
                octocode_runtime_base::test_support::BUILD_IDENTITY,
                &query,
                &policy,
                &ContentSecurity::new(),
                &NeverCancel,
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
    let scope = octocode_tool_lsp::lsp_search::scope::Scope::new(
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
