//! What a served row states of its hits and its scope: the declaration of
//! every hit a merged window holds, what a narrowing filter kept out of an
//! absence, the per-term counts of a count view, and the totals a walk's
//! last page states. Read from the call's served result.
use super::*;
use octocode_runtime_base::id::ToolId;
use serde_json::{Value, json};

/// One localSearch call over `root` with `fields`, as an agent receives it:
/// the row staged as the runtime stages it, held to the output contract.
fn served(root: &std::path::Path, fields: Value) -> Value {
    let (policy, security) = policy_for(root);
    let result = search_in(&ls_query(fields, None), &policy, &security, None).expect("search");
    stage(&result)
}

/// `result` as an agent receives it (an empty row says so), held to the
/// output contract.
fn stage(result: &LocalSearchResult) -> Value {
    let mut data = result.to_row().expect("row");
    if result.status == SearchStatus::Empty {
        data["status"] = json!("empty");
    }
    let out = octocode_runtime_base::test_support::staged_response(ToolId::LocalSearch, vec![data]);
    octocode_runtime_base::contracts::validate_output("localSearch", &out)
        .unwrap_or_else(|error| panic!("{error}\n{out}"));
    assert_ne!(out["results"][0]["status"], "error", "{out}");
    out
}

fn data(out: &Value) -> &Value {
    &out["results"][0]["data"]
}

/// Three functions, one hit in each, close enough that three context lines
/// around each hit join them into one window.
const THREE_FNS: &str = "fn alpha() {\n    let a = ZETA;\n}\n\n\nfn beta() {\n    let b = ZETA;\n}\n\n\nfn gamma() {\n    let c = ZETA;\n}\n";

/// A declaration's name: the served row states it as an object or, once the
/// response stage forms it (Q5), as its line-first outline string.
fn declaration_name(declaration: &Value) -> String {
    let row = match declaration.as_str() {
        Some(text) => {
            octocode_runtime_base::response::symbol_outline::outline_row(text)
                .expect("an outline line")
                .1
        }
        None => declaration.clone(),
    };
    row["symbolName"]
        .as_str()
        .expect("a declaration")
        .to_owned()
}

/// The declaration names a row's hits name, in the order of its matched
/// lines.
fn owners(file: &Value, row: &Value) -> Vec<String> {
    let name = |index: &Value| {
        let index = usize::try_from(index.as_u64().expect("an index")).expect("index");
        declaration_name(&file["declarations"][index])
    };
    match &row["enclosing"] {
        Value::Array(indexes) => indexes.iter().map(name).collect(),
        Value::Null => Vec::new(),
        index => vec![name(index)],
    }
}

/// One page of a search over `root` with `fields`, as its row: a page with a
/// next page, whose cursor this crate does not issue.
fn row_of(root: &std::path::Path, fields: Value) -> Value {
    let (policy, security) = policy_for(root);
    let result = search_in(&ls_query(fields, None), &policy, &security, None).expect("search");
    result.to_row().expect("row")
}

/// A merged window names the declaration of each hit it holds, aligned with
/// its matched lines, and the file lists each of them; a page that ends
/// inside the window names the declarations of the hits it shows.
#[test]
fn a_merged_window_names_the_declaration_of_every_hit_it_holds() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("real root");
    fs::create_dir_all(root.join("src")).expect("dir");
    fs::write(root.join("src/two.rs"), THREE_FNS).expect("fixture");
    for (limit, expected) in [
        (None, vec!["alpha", "beta", "gamma"]),
        (Some(2), vec!["alpha", "beta"]),
    ] {
        let mut fields = json!({
            "path": root.join("src"),
            "matchString": ["ZETA"],
            "contextLines": {"before": 3, "after": 3}
        });
        let out = match limit {
            Some(limit) => {
                fields["limit"] = json!(limit);
                json!({"results": [{"data": row_of(&root, fields)}]})
            }
            None => served(&root, fields),
        };
        let file = &data(&out)["files"][0];
        let rows = file["matches"].as_array().expect("rows");
        assert_eq!(rows.len(), 1, "one merged window: {out}");
        let lines = rows[0]["matchedLines"].as_array().expect("merged lines");
        assert_eq!(lines.len(), expected.len(), "{out}");
        assert_eq!(owners(file, &rows[0]), expected, "{out}");
        let listed = file["declarations"]
            .as_array()
            .expect("declarations")
            .iter()
            .map(declaration_name)
            .collect::<Vec<_>>();
        assert_eq!(listed, expected, "{out}");
    }
}

/// A row whose hits all sit in one declaration names it once, as an index.
#[test]
fn a_merged_window_inside_one_declaration_names_it_once() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("real root");
    fs::write(
        root.join("one.rs"),
        "fn alpha() {\n    let a = ZETA;\n    let b = ZETA;\n}\n",
    )
    .expect("fixture");
    let out = served(
        &root,
        json!({"path": root, "matchString": ["ZETA"], "contextLines": {"before": 1, "after": 1}}),
    );
    let row = &data(&out)["files"][0]["matches"][0];
    assert!(row["matchedLines"].is_array(), "{out}");
    assert!(row["enclosing"].is_u64(), "{out}");
}

/// A count view over two terms states each file's count per term on its
/// first page, as on any other.
#[test]
fn a_two_term_count_view_states_term_counts_on_its_first_page() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("real root");
    fs::write(root.join("a.txt"), "alpha beta\nalpha\n").expect("fixture");
    fs::write(root.join("b.txt"), "beta beta\n").expect("fixture");
    // The second pair holds a term no file holds, which counts zero.
    for (view, terms) in [
        ("countMatches", json!(["alpha", "beta"])),
        ("countLines", json!(["alpha", "beta"])),
        ("countMatches", json!(["beta", "zzq_absent\\b"])),
    ] {
        let out = served(
            &root,
            json!({"path": root, "matchString": terms, "view": view}),
        );
        let files = data(&out)["files"].as_array().expect("files");
        assert!(!files.is_empty(), "{view}: {out}");
        for file in files {
            let counts = file["termCounts"]
                .as_array()
                .unwrap_or_else(|| panic!("{view}: each file states its term counts: {out}"));
            assert_eq!(counts.len(), 2, "{view}: {out}");
        }
    }
}

/// Two literal terms that overlap in a line each count their own matches, as
/// `rg -o -F` per term counts them: the term found only inside the other's
/// match counts too.
#[test]
fn overlapping_literal_terms_count_as_each_alone_counts() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("real root");
    fs::write(
        root.join("a.rs"),
        "m.insert(\"snapshot\".into(), v);\nm.insert(\"snapshot\", w);\n",
    )
    .expect("fixture");
    let out = served(
        &root,
        json!({"path": root, "matchString": ["\"snapshot\".into()", "insert(\"snapshot\""], "view": "countMatches"}),
    );
    let files = data(&out)["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{out}");
    assert_eq!(files[0]["termCounts"], json!([1, 2]), "{out}");
}

/// Every page of a walk from `fields`, each as its row, following its
/// `next.nextPage`; the last one also as served.
fn walk(root: &std::path::Path, fields: Value) -> (Vec<Value>, Value) {
    let (policy, security) = policy_for(root);
    let mut query = fields;
    let mut rows = Vec::new();
    loop {
        let result = search_in(&ls_query(query, None), &policy, &security, None).expect("page");
        let row = result.to_row().expect("row");
        let next = row["next"]["nextPage"]["input"]["queries"][0].clone();
        rows.push(row);
        if next.is_null() {
            return (rows, stage(&result));
        }
        assert!(rows.len() < 50, "the walk ends");
        query = next;
    }
}

/// A walk over several pages states the files it walked so far on every
/// page and, on its last, the search's totals: the same as one page that
/// holds the whole walk states, whether its pages end between files or
/// inside one.
#[test]
fn a_walk_states_files_walked_so_far_and_its_totals_on_its_last_page() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("real root");
    // Three files without a hit walk first; then twelve with two hit lines
    // and three occurrences each.
    for file in 0..3 {
        fs::write(root.join(format!("e{file:02}.txt")), "nothing\n").expect("fixture");
    }
    for file in 0..12 {
        fs::write(
            root.join(format!("f{file:02}.txt")),
            "needle needle\nother\nneedle\n",
        )
        .expect("fixture");
    }
    let start = json!({"path": root, "matchString": ["needle"]});
    let (whole, _) = walk(&root, start.clone());
    assert_eq!(whole.len(), 1, "one page holds the walk: {whole:?}");
    let totals = |row: &Value| {
        (
            row["matchCount"].clone(),
            row.get("matchedLineCount").cloned(),
            row["matchedFiles"].clone(),
            row["filesScanned"].clone(),
        )
    };
    assert_eq!(
        totals(&whole[0]),
        (json!(36), Some(json!(24)), json!(12), json!(15)),
        "{}",
        whole[0]
    );
    for limit in [3, 4, 5] {
        let mut fields = start.clone();
        fields["limit"] = json!(limit);
        let (pages, last) = walk(&root, fields);
        assert!(pages.len() > 2, "limit {limit}: {pages:?}");
        let mut walked = 0;
        for page in &pages[..pages.len() - 1] {
            let scanned = page["filesScanned"].as_u64().expect("filesScanned");
            assert!(
                scanned >= walked,
                "limit {limit}: files walked so far: {pages:?}"
            );
            walked = scanned;
            assert!(page.get("matchCount").is_none(), "limit {limit}: {page}");
        }
        assert_eq!(
            totals(&data(&last).clone()),
            totals(&whole[0]),
            "limit {limit}: the last page states the walk's totals: {last}"
        );
    }
}

/// A finished walk lists what it skipped with no `sampled` key: absent
/// means every skipped path is listed.
#[test]
fn a_finished_walk_writes_no_sampled_key() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("real root");
    fs::write(root.join(".gitignore"), "build/\n").expect("fixture");
    fs::create_dir_all(root.join(".git")).expect("git dir");
    fs::create_dir_all(root.join("build")).expect("dir");
    fs::write(root.join("build/out.txt"), "needle\n").expect("fixture");
    fs::write(root.join("a.txt"), "needle\n").expect("fixture");
    let (policy, security) = policy_for(&root);
    let searched = served(&root, json!({"path": root, "matchString": ["needle"]}));
    let listed = list_structure(
        json!({"operation": "tree", "path": root}),
        &policy,
        &security,
    );
    for out in [data(&searched).to_string(), listed] {
        assert!(out.contains("skippedPaths"), "{out}");
        assert!(!out.contains("sampled"), "{out}");
    }
}

/// The declaration rows a row's `declaration` names, in order.
fn declared(file: &Value, row: &Value) -> Vec<Value> {
    let listed = |index: &Value| {
        let index = usize::try_from(index.as_u64().expect("an index")).expect("index");
        let declaration = &file["declarations"][index];
        match declaration.as_str() {
            Some(text) => {
                octocode_runtime_base::response::symbol_outline::outline_row(text)
                    .expect("an outline line")
                    .1
            }
            None => declaration.clone(),
        }
    };
    match &row["declaration"] {
        Value::Array(indexes) => indexes.iter().map(listed).collect(),
        Value::Null => Vec::new(),
        index => vec![listed(index)],
    }
}

/// A function holding two local bindings, a top-level constant and a class
/// with a method.
const DECLARING_TS: &str = "export function outer(a: number) {\n  const inner = a + 1;\n  let other = inner;\n  return other;\n}\nexport const top = 1;\nclass Box {\n  run() { return 1; }\n}\n";

/// P5: a hit on a declared name names that declaration by index, and its
/// row states where it ends, so the read after it needs no guessed range.
/// `enclosing` stays the declaration around the hit. The outline is the one
/// source: a hit it does not list (a local binding, a file of a language
/// with no outline) carries no `declaration`.
#[test]
fn a_declaring_hit_names_its_own_declaration_and_where_it_ends() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("real root");
    fs::write(root.join("a.ts"), DECLARING_TS).expect("fixture");
    fs::write(root.join("b.txt"), "function plain() {\n}\nlet y = 2\n").expect("fixture");
    let out = served(
        &root,
        json!({
            "path": root,
            "matchString": ["outer", "inner", "run", "plain", "y"],
            "wholeWord": true
        }),
    );
    let files = data(&out)["files"].as_array().expect("files");
    let file = |name: &str| {
        files
            .iter()
            .find(|file| {
                file["path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with(name))
            })
            .unwrap_or_else(|| panic!("{name}: {out}"))
    };
    let source = file("a.ts");
    let row = |line: u64| {
        source["matches"]
            .as_array()
            .expect("rows")
            .iter()
            .find(|row| row["line"] == line)
            .unwrap_or_else(|| panic!("line {line}: {out}"))
    };
    let spans = |row: &Value| {
        declared(source, row)
            .iter()
            .map(|d| {
                (
                    d["symbolName"].as_str().expect("name").to_owned(),
                    d["line"].as_u64().expect("line"),
                    d["endLine"]
                        .as_u64()
                        .unwrap_or_else(|| d["line"].as_u64().expect("line")),
                )
            })
            .collect::<Vec<_>>()
    };
    // The function's name line: its own declaration and its last line.
    assert_eq!(spans(row(1)), [("outer".to_owned(), 1, 5)], "{out}");
    assert!(row(1).get("enclosing").is_none(), "{out}");
    // A method: its own declaration, inside its class.
    assert_eq!(spans(row(8)), [("run".to_owned(), 8, 8)], "{out}");
    assert_eq!(owners(source, row(8)), ["Box"], "{out}");
    // A local binding the outline does not list declares nothing here; a
    // use is no declaration either.
    assert!(row(2).get("declaration").is_none(), "{out}");
    assert_eq!(owners(source, row(2)), ["outer"], "{out}");
    assert!(row(3).get("declaration").is_none(), "{out}");
    // A file without an outline: no declaration to point at.
    let text = file("b.txt");
    assert!(text.get("declarations").is_none(), "{out}");
    for row in text["matches"].as_array().expect("rows") {
        assert!(row.get("declaration").is_none(), "{out}");
    }
}

/// A merged window holding several declaring hits names each one's
/// declaration, in hit order; each declaration's name line is its hit line.
#[test]
fn a_merged_window_names_every_declaration_its_hits_declare() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("real root");
    fs::write(root.join("two.rs"), THREE_FNS).expect("fixture");
    let out = served(
        &root,
        json!({
            "path": root,
            "matchString": ["alpha", "beta", "gamma"],
            "contextLines": {"before": 3, "after": 3}
        }),
    );
    let file = &data(&out)["files"][0];
    let rows = file["matches"].as_array().expect("rows");
    assert_eq!(rows.len(), 1, "one merged window: {out}");
    let lines = rows[0]["matchedLines"].as_array().expect("merged lines");
    let named = declared(file, &rows[0]);
    assert_eq!(
        named.iter().map(|d| d["line"].clone()).collect::<Vec<_>>(),
        *lines,
        "{out}"
    );
    assert_eq!(
        named
            .iter()
            .map(|d| d["symbolName"].as_str())
            .collect::<Vec<_>>(),
        [Some("alpha"), Some("beta"), Some("gamma")],
        "{out}"
    );
}

/// A declaration search's hit starts at the head's first word: it still
/// names the declaration the head declares.
#[test]
fn a_declaration_search_names_the_declaration_it_found() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("real root");
    fs::write(root.join("two.rs"), THREE_FNS).expect("fixture");
    let out = served(&root, json!({"path": root, "matchString": ["fn beta"]}));
    let file = &data(&out)["files"][0];
    let rows = file["matches"].as_array().expect("rows");
    assert_eq!(rows.len(), 1, "{out}");
    let named = declared(file, &rows[0]);
    assert_eq!(named.len(), 1, "{out}");
    assert_eq!(named[0]["symbolName"], "beta", "{out}");
    assert_eq!(named[0]["line"], rows[0]["line"], "{out}");
    assert_eq!(named[0]["endLine"], 8, "{out}");
}
