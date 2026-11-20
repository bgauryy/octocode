//! astSearch pages as a runtime call delivers them, through the response
//! stages that respell paths ([`published`]). The page budget depends on
//! the response window alone, so page 1 (an absolute `path`) and the
//! continuation it publishes (that path workspace-relative) cut alike;
//! rows under a deep scope are sized by the path the response prints, so
//! a page fits the window it was cut for.
use super::{AstSearchQuery, execute_ast};
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::test_support::{
    Fixture, checks, published, published_next_page, simple_policy,
};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

const WINDOW: usize = 16_384;

const PATTERN: &str = "export function $NAME(value: number): number { $$$BODY }";

/// `base` nested `depth` levels of 60-char directory names.
fn deep_dir(base: &str, depth: usize) -> String {
    (0..depth).fold(base.to_owned(), |dir, level| {
        let mut name = format!("level_{level}_{}", "x".repeat(60));
        name.truncate(60);
        format!("{dir}/{name}")
    })
}

fn typed(row: &Value) -> AstSearchQuery {
    let validated =
        octocode_runtime_base::contracts::validate_built_query("astSearch", row.clone())
            .unwrap_or_else(|error| panic!("{row}: {error:?}"));
    serde_json::from_value(validated).expect("typed row")
}

fn functions(prefix: &str, count: usize) -> String {
    (0..count)
        .map(|index| {
            format!("export function {prefix}_{index:04}(value: number): number {{ return value + {index}; }}\n")
        })
        .collect()
}

/// `files` sources under `dir` of `root`, `per_file` functions each.
fn sources(root: &Fixture, dir: &str, files: usize, per_file: usize) {
    for file in 0..files {
        root.write(
            &format!("{dir}/file_{file:03}.ts"),
            functions(&format!("declared_{file:03}"), per_file),
        );
    }
}

/// Every astSearch mode run from an absolute deep path publishes a
/// `next.nextPage` (the path respelled workspace-relative) that validates
/// as a query of the tool.
#[test]
fn every_mode_publishes_a_valid_page_two_from_a_deep_path() {
    let root = Fixture::new();
    let dir = deep_dir("walks", 3);
    sources(&root, &format!("{dir}/many"), 500, 1);
    root.write(&format!("{dir}/big.ts"), functions("big", 1_500));
    // The canonical spelling the response stage respells (a temp dir may
    // sit behind a symlink, as `/var` on macOS).
    let base = root.0.canonicalize().expect("canonical root");
    let (paths, security) = simple_policy(&base);
    let many = base.join(format!("{dir}/many"));
    let big = base.join(format!("{dir}/big.ts"));
    let rows = [
        json!({"operation":"symbols","path":many}),
        json!({"operation":"match","rule":{"pattern":PATTERN},"parser":"typescript","path":many}),
        json!({"operation":"match","rule":{"pattern":PATTERN},"parser":"typescript","path":many}),
        json!({"operation":"syntaxTree","path":big}),
    ];
    for row in rows {
        assert!(row["path"].as_str().is_some_and(|path| path.len() > 128));
        let out = execute_ast(
            octocode_runtime_base::test_support::BUILD_IDENTITY,
            &typed(&row),
            &paths,
            &security,
            &NeverCancel,
            Some(WINDOW),
        )
        .unwrap_or_else(|error| panic!("{row}: {error:?}"));
        assert_eq!(out["pagination"]["hasMore"], true, "{row} spans pages");
        let next = published_next_page(ToolId::AstSearch, &out);
        octocode_runtime_base::contracts::validate_built_query("astSearch", next.clone())
            .unwrap_or_else(|error| panic!("{next}: {error:?}"));
    }
}

/// Walks `row` through every page lead (`checks::walk`: each call once,
/// each row once), each page published as a runtime call delivers it, and
/// returns the delivered pages.
fn delivered_walk(root: &Fixture, row: Value, rows: fn(&Value) -> Vec<String>) -> Vec<Value> {
    let (paths, security) = simple_policy(&root.0);
    let mut pages = Vec::new();
    let walk = checks::walker::walk("astSearch", json!({"queries":[row]}), |call| {
        let input = octocode_runtime_base::test_support::opened_input(ToolId::AstSearch, call);
        let row = octocode_runtime_base::contracts::validate_built_query(
            "astSearch",
            input["queries"][0].clone(),
        )
        .expect("a published lead validates");
        let out = execute_ast(
            octocode_runtime_base::test_support::BUILD_IDENTITY,
            &typed(&row),
            &paths,
            &security,
            &NeverCancel,
            Some(WINDOW),
        )
        .unwrap_or_else(|error| panic!("{row}: {error:?}"));
        let output = published(ToolId::AstSearch, &out);
        pages.push(output.clone());
        checks::Page {
            rows: rows(&output["results"][0]["data"]),
            output,
        }
    });
    assert!(walk.queries.len() > 1, "the fixture spans pages");
    pages
}

/// `path name` of each row in a delivered page's files (`list` rows named
/// by `key`), or `path` of each listed file when `list` is empty.
fn file_rows(data: &Value, list: &str, key: &str) -> Vec<String> {
    let files = data["files"].as_array().into_iter().flatten();
    files
        .flat_map(|file| {
            let path = file["path"].as_str().expect("file path").to_owned();
            if list.is_empty() {
                return vec![path];
            }
            file[list]
                .as_array()
                .into_iter()
                .flatten()
                .map(|row| format!("{path} {}", row[key]))
                .collect()
        })
        .collect()
}

fn symbol_rows(data: &Value) -> Vec<String> {
    file_rows(data, "symbols", "symbolName")
}

fn match_rows(data: &Value) -> Vec<String> {
    file_rows(data, "matches", "line")
}

fn listed_files(data: &Value) -> Vec<String> {
    file_rows(data, "", "")
}

/// Under a deep scope the response prints each file's path
/// workspace-relative, far longer than the scope-rooted path the tool
/// writes: pages sized by the printed path fit the window (the response
/// pager never splits them) and still fill it, and a walk reaches every row
/// once.
/// The row ids of one page of a walk.
type RowIds = fn(&Value) -> Vec<String>;

#[test]
fn deep_scope_pages_measure_row_paths_as_delivered() {
    const FILES: usize = 300;
    // Only the scan's own budget may cut it, never a loaded machine's clock.
    let _frozen = octocode_engine::scan_budget::freeze_walk_clock();
    let root = Fixture::new();
    let dir = deep_dir("deep", 5);
    sources(&root, &format!("{dir}/src"), FILES, 1);
    let scope = root.0.join(format!("{dir}/src"));
    let walks: [(Value, RowIds); 3] = [
        (json!({"operation":"symbols","path":scope}), symbol_rows),
        (
            json!({"operation":"match","rule":{"pattern":PATTERN},"parser":"typescript","path":scope}),
            match_rows,
        ),
        (
            json!({"operation":"match","rule":{"pattern":PATTERN},"parser":"typescript","path":scope,"view":"files"}),
            listed_files,
        ),
    ];
    for (row, rows) in walks {
        let pages = delivered_walk(&root, row.clone(), rows);
        let mut seen = 0;
        for (index, page) in pages.iter().enumerate() {
            let data = &page["results"][0]["data"];
            let path = data["files"][0]["path"].as_str().expect("a file row");
            assert!(path.len() > 300, "rows print the deep path: {path}");
            assert!(!rows(data).is_empty(), "page {index} carries evidence");
            seen += rows(data).len();
        }
        assert_eq!(seen, FILES, "{row}: every row once");
    }
}

/// A relational rule's page two keeps the rule the caller sent: a generated
/// rule type writes an unset boxed sub-rule (`follows`, `has`, `not`) as
/// `null`, which no query admits, so the published `next.nextPage` must leave
/// it out, satisfy the output contract and run as page two of the same rule.
#[test]
fn a_relational_rule_publishes_a_page_two_that_validates_and_runs() {
    let root = Fixture::new();
    root.write(
        "src/lib.rs",
        "fn f() { let a = || g(1); let b = || g(2); let c = || g(3); }\n",
    );
    let base = root.0.canonicalize().expect("canonical root");
    let (paths, security) = simple_policy(&base);
    let rule = json_fixture(
        r#"{
            "kind": "call_expression",
            "inside": {"kind": "closure_expression", "stopBy": "end"}
        }"#,
    );
    let row = json!({"operation":"match","path":base.join("src"),"rule":rule,"limit":1});
    let out = execute_ast(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        &typed(&row),
        &paths,
        &security,
        &NeverCancel,
        None,
    )
    .unwrap_or_else(|error| panic!("{row}: {error:?}"));
    assert_eq!(out["pagination"]["hasMore"], true, "{out}");
    octocode_runtime_base::test_support::assert_row_contract(ToolId::AstSearch, &out);
    let next = published_next_page(ToolId::AstSearch, &out);
    assert_eq!(
        next["rule"], rule,
        "page two keeps the rule as sent: {next}"
    );
    let next = octocode_runtime_base::contracts::validate_built_query("astSearch", next.clone())
        .unwrap_or_else(|error| panic!("{next}: {error:?}"));
    let page_two = execute_ast(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        &typed(&next),
        &paths,
        &security,
        &NeverCancel,
        None,
    )
    .unwrap_or_else(|error| panic!("{next}: {error:?}"));
    octocode_runtime_base::test_support::assert_row_contract(ToolId::AstSearch, &page_two);
}

/// The page count a first page's narrowing note states, if it has one.
fn stated_pages(data: &Value) -> Option<usize> {
    data["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .find_map(|warning| {
            let (_, rest) = warning.split_once(" fill ")?;
            rest.split_whitespace().next()?.parse().ok()
        })
}

/// Page 1 carries the scan's census beside its rows (the skipped paths,
/// the files no grammar reads, the skipped-grammar counts, the narrowing
/// note), which no later page repeats: the cut charges it to page 1, so
/// every page of the walk, as delivered, fits the response window the
/// response pager would otherwise split into row parts, the walk takes as
/// many calls as page 1 says it fills, and every file comes once.
#[test]
fn a_first_page_with_its_census_fits_the_window_and_states_the_walks_calls() {
    let root = Fixture::new();
    let long = "z".repeat(60);
    sources(&root, "src", 600, 2);
    // Named one by one in `skippedPaths`, and three of them in the note.
    for file in 0..octocode_tools_shared::leads::skipped::NAMED_UNPARSED {
        root.write(&format!("data/blob_{file:02}_{long}.unknownext"), "x\n");
    }
    for file in 0..5 {
        root.write(&format!("py/module_{file}.py"), "def f():\n    pass\n");
    }
    let base = root.0.canonicalize().expect("canonical root");
    let (paths, security) = simple_policy(&base);
    // A ranked file view reads the whole scope before page 1 and states its
    // page count; a walk-order content view pages scan windows, which
    // count no final total and state none.
    let views = [
        (
            json!({"operation":"match","path":base,"rule":{"kind":"function_declaration"},"view":"countMatches","sort":{"by":"matchCount","order":"desc"}}),
            true,
        ),
        (
            json!({"operation":"match","path":base,"rule":{"kind":"function_declaration"}}),
            false,
        ),
    ];
    for (row, counted) in views {
        let mut pending = Some(row.clone());
        let (mut calls, mut stated, mut files) = (0, None, std::collections::BTreeSet::new());
        while let Some(query) = pending.take() {
            let out = execute_ast(
                octocode_runtime_base::test_support::BUILD_IDENTITY,
                &typed(&query),
                &paths,
                &security,
                &NeverCancel,
                Some(WINDOW),
            )
            .unwrap_or_else(|error| panic!("{query}: {error:?}"));
            let served = published(ToolId::AstSearch, &out);
            calls += 1;
            assert!(calls <= 50, "nonterminating walk: {row}");
            let size = octocode_engine::json_size::utf16_units(&served).expect("size");
            assert!(size <= WINDOW, "page {calls} of {row}: {size} > {WINDOW}");
            let data = &served["results"][0]["data"];
            if calls == 1 {
                assert!(data["skippedPaths"].is_object(), "the census: {data}");
                assert!(data["skippedGrammars"].is_object(), "the census: {data}");
                stated = stated_pages(data);
            }
            for file in data["files"].as_array().into_iter().flatten() {
                let path = file["path"].as_str().expect("path").to_owned();
                // A content walk lists a file once per page its matches are on.
                if file.get("matches").is_none() {
                    assert!(files.insert(path), "a file repeats: {row}");
                } else {
                    files.insert(path);
                }
            }
            pending = data["next"]["nextPage"]
                .is_object()
                .then(|| published_next_page(ToolId::AstSearch, &out));
        }
        assert!(calls > 1, "the fixture spans pages: {row}");
        assert_eq!(
            stated,
            counted.then_some(calls),
            "page 1 states the walk's calls: {row}"
        );
        assert_eq!(files.len(), 600, "every file once: {row}");
    }
}
