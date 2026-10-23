//! Enclosing declarations: named once per page, unambiguous, and leading to
//! callers; globs and includes that reach nothing are named.

use super::*;

/// Rows in one declaration name it once: the first carries `enclosing`,
/// the rest `enclosingLine` (its name line), and every row still resolves
/// to its owner. The page is smaller than naming it on every row.
#[test]
fn rows_in_one_declaration_name_it_once_and_the_rest_reference_it() {
    let source = "fn f() {\n    helper();\n    helper();\n    helper();\n}\n\nfn g() {\n    helper();\n}\n\nfn h() {\n    helper();\n    helper();\n}\n";
    let body = search_fixture(
        &[("src/a.rs", source)],
        ls_query(serde_json::json!({"matchString": "helper"}), None),
    );
    let file = &body["files"][0];
    let rows = file["matches"].as_array().expect("rows");
    assert_eq!(rows.len(), 6, "{body}");
    let named: Vec<bool> = rows
        .iter()
        .map(|row| row.get("enclosing").is_some())
        .collect();
    assert_eq!(named, [true, false, false, true, true, false], "{body}");
    assert_eq!(rows[1]["enclosingLine"], 1, "{body}");
    assert_eq!(rows[2]["enclosingLine"], 1, "{body}");
    assert_eq!(rows[5]["enclosingLine"], 11, "{body}");
    assert!(rows[0].get("enclosingLine").is_none(), "{body}");
    let owners: Vec<String> = (0..rows.len())
        .map(|i| {
            owner_of(file, i).expect("owner")["symbolName"]
                .as_str()
                .expect("name")
                .to_owned()
        })
        .collect();
    assert_eq!(owners, ["f", "f", "f", "g", "h", "h"], "{body}");
    // Bytes: one full object per declaration, a number for each repeat.
    let full = r#""enclosing":{"symbolName":"f","kind":"function","line":1,"endLine":5}"#.len();
    let bare: usize = rows
        .iter()
        .map(|row| {
            let mut row = row.clone();
            row.as_object_mut()
                .expect("row")
                .retain(|key, _| !key.starts_with("enclosing"));
            row.to_string().len()
        })
        .sum();
    let shown: usize = rows.iter().map(|row| row.to_string().len()).sum();
    // Naming it on every row would add 6 full objects; 3 repeats are a
    // number instead (about 50 bytes each).
    let every_row = bare + 6 * (full + 1);
    assert!(shown + 100 < every_row, "{shown} vs {every_row}: {body}");
    // A sweep names its owner once, then references it 59 times.
    let sweep = format!("fn many() {{\n{}}}\n", "    helper();\n".repeat(60));
    let wide = search_fixture(
        &[("src/b.rs", &sweep)],
        ls_query(serde_json::json!({"matchString": "helper"}), None),
    );
    let text = wide["files"][0]["matches"].to_string();
    assert_eq!(text.matches("\"enclosing\":").count(), 1, "{wide}");
    assert_eq!(text.matches("\"enclosingLine\":1").count(), 59, "{wide}");
}

/// Two declarations sharing a name line (an impl and its first method)
/// stay distinguishable: a reference means the nearest earlier row naming
/// that line, so a row whose owner is not that one names its own.
#[test]
fn enclosing_references_stay_unambiguous_when_name_lines_collide() {
    let source = "impl X { fn a() {\n    helper();\n}\n    const C: u8 = helper();\n    const D: u8 = helper();\n}\n";
    let body = search_fixture(
        &[("src/c.rs", source)],
        ls_query(serde_json::json!({"matchString": "helper"}), None),
    );
    let file = &body["files"][0];
    let rows = file["matches"].as_array().expect("rows");
    assert_eq!(rows.len(), 3, "{body}");
    let owners: Vec<serde_json::Value> =
        (0..3).map(|i| owner_of(file, i).expect("owner")).collect();
    assert_eq!(owners[0]["symbolName"], "a", "{body}");
    assert_eq!(owners[1]["symbolName"], "X", "{body}");
    assert_eq!(owners[2]["symbolName"], "X", "{body}");
    // Row 1 shares name line 1 with row 0's owner but is another declaration.
    assert!(rows[1].get("enclosing").is_some(), "{body}");
    assert_eq!(rows[2]["enclosingLine"], 1, "{body}");
}

/// `include` naming a directory path scopes the search to its files
/// (it used to match nothing: filesScanned 0 read as "no text match").
#[test]
fn a_directory_path_include_searches_its_files() {
    let body = search_fixture(
        &[("pkg/src/a.ts", "needle\n"), ("pkg/test/b.ts", "needle\n")],
        ls_query(
            serde_json::json!({"matchString": "needle", "include": ["pkg/src"]}),
            None,
        ),
    );
    let text = body.to_string();
    assert!(text.contains("pkg/src/a.ts"), "{body}");
    assert!(!text.contains("pkg/test/b.ts"), "{body}");
}

/// Globs match paths under `path`, so a `..` segment never matches: an
/// include that reaches up searches nothing, and an exclude that does
/// excludes nothing. Both name the glob.
#[test]
fn a_glob_that_reaches_above_path_is_named() {
    for (glob, key) in [("../src/*.ts", "include"), ("../pkg/**", "exclude")] {
        let body = search_fixture(
            &[("pkg/src/a.ts", "needle\n")],
            ls_query(
                serde_json::json!({"matchString": "needle", key: [glob]}),
                None,
            ),
        );
        let warnings = body["warnings"].to_string();
        assert!(
            warnings.contains(glob) && warnings.contains(".."),
            "{key} {glob}: {body}"
        );
    }
}

/// The listing lead after a `..` include leaves the glob out: re-sending it
/// to structureSearch would list nothing again.
#[test]
fn the_listing_lead_leaves_out_a_glob_that_reaches_above_path() {
    let request = ls_query(
        serde_json::json!({"path": "/repo/pkg", "matchString": "needle", "include": ["../src/*.ts", "*.md"]}),
        None,
    );
    let listing = leads::scope_listing(&request, true);
    let listing = &listing["query"]["queries"][0];
    assert_eq!(listing["include"], serde_json::json!(["*.md"]), "{listing}");
}

/// When include/exclude leave no file to search, the row says the scope
/// matched nothing instead of suggesting a different spelling of the text.
#[test]
fn an_include_that_matches_no_file_says_so() {
    let body = search_fixture(
        &[("pkg/src/a.ts", "needle\n")],
        ls_query(
            serde_json::json!({"matchString": "Needle", "include": ["nowhere/dir"]}),
            None,
        ),
    );
    assert_eq!(body["filesScanned"], 0, "{body}");
    let hint = body["hints"][0].as_str().expect("scope hint");
    assert!(hint.contains("include/exclude matched no file"), "{hint}");
    // The next call is named, and the row carries it.
    assert!(hint.contains("viewTree"), "{hint}");
    assert_eq!(
        body["next"]["viewTree"]["tool"], "structureSearch",
        "{body}"
    );
    assert!(
        hint.chars().count() <= 120,
        "clipped by the response stage: {hint}"
    );
    crate::tools::test_support::checks::assert_absent(&body["hints"], "all-lowercase");
}

/// A small page names each hit's enclosing declaration (a definition hit
/// names the one around it), and a searched symbol declared on a hit
/// line yields an lspSearch lead anchored on that line (`callers` for a
/// function) when a language server would start for the file. A sweep of
/// more hits than one small page stays plain lines.
#[test]
fn small_pages_name_enclosing_declarations_and_lead_to_callers() {
    let source = "struct Harness;\nimpl Harness {\n    fn try_read_output(&self) {\n        helper();\n    }\n}\n\nfn helper() {}\n\nfn caller() {\n    helper();\n}\n";
    let body = search_fixture(
        &[("src/a.rs", source)],
        ls_query(serde_json::json!({"matchString": "helper"}), None),
    );
    let rows = &body["files"][0]["matches"];
    assert_eq!(rows[0]["line"], 4, "{body}");
    assert_eq!(
        rows[0]["enclosing"],
        serde_json::json!({"symbolName":"try_read_output","kind":"method","line":3,"endLine":5}),
        "{body}"
    );
    assert_eq!(rows[1]["line"], 8, "{body}");
    assert!(rows[1].get("enclosing").is_none(), "{body}");
    assert_eq!(
        rows[2]["enclosing"],
        serde_json::json!({"symbolName":"caller","kind":"function","line":10,"endLine":12}),
        "{body}"
    );
    let lead = &body["next"]["findCallers"];
    let declared_at = body["files"][0]["path"].as_str().expect("path").to_owned();
    let server = crate::tools::leads::lsp::verify_query(
        &declared_at,
        "helper",
        8,
        crate::tools::leads::lsp::Verify::Callers,
    );
    if server.is_none() {
        assert!(lead.is_null(), "no language server, no lead: {body}");
        return;
    }
    assert_eq!(lead["tool"], "lspSearch", "{body}");
    assert_eq!(
        lead["query"]["queries"][0]["operation"], "callers",
        "{body}"
    );
    assert_eq!(
        lead["query"]["queries"][0]["symbolName"], "helper",
        "{body}"
    );
    assert_eq!(lead["query"]["queries"][0]["line"], 8, "{body}");
    let uri = lead["query"]["queries"][0]["path"].as_str().expect("path");
    assert!(
        std::path::Path::new(uri).is_absolute() && uri.ends_with("src/a.rs"),
        "{uri}"
    );

    let keyword = search_fixture(
        &[("src/a.rs", source)],
        ls_query(serde_json::json!({"matchString": "fn helper"}), None),
    );
    assert_eq!(
        keyword["next"]["findCallers"]["query"]["queries"][0]["line"], 8,
        "{keyword}"
    );
    let insensitive = search_fixture(
        &[("src/a.rs", source)],
        ls_query(
            serde_json::json!({"matchString": "helper", "caseMode": "insensitive"}),
            None,
        ),
    );
    assert!(
        insensitive["next"].get("callers").is_none(),
        "{insensitive}"
    );

    // Every hit in one declaration names it (a later row without it
    // read as "no enclosing function"); a hit outside any has none.
    let run =
        "fn f() {\n    helper();\n    helper();\n    helper();\n}\nstatic H: fn() = helper;\n";
    let body = search_fixture(
        &[("src/r.rs", run)],
        ls_query(serde_json::json!({"matchString": "helper"}), None),
    );
    let rows = &body["files"][0]["matches"];
    for row in 0..3 {
        assert_eq!(
            owner_of(&body["files"][0], row),
            Some(serde_json::json!({"symbolName":"f","kind":"function","line":1,"endLine":5})),
            "{body}"
        );
    }
    assert_eq!(rows[3]["line"], 6, "{body}");
    assert!(rows[3].get("enclosing").is_none(), "{body}");

    // A declaration search names the owner of declaring hits only: the
    // other hits are text that merely mentions the declaration.
    let mentions = "impl Harness {\n    fn try_read_output(&self) {}\n}\nfn caller() {\n    let s = \"fn try_read_output\";\n}\n";
    let body = search_fixture(
        &[("src/m.rs", mentions)],
        ls_query(
            serde_json::json!({"matchString": "fn try_read_output"}),
            None,
        ),
    );
    let rows = &body["files"][0]["matches"];
    assert_eq!(
        rows[0]["enclosing"],
        serde_json::json!({"symbolName":"Harness","kind":"impl","line":1,"endLine":3}),
        "{body}"
    );
    assert_eq!(rows[1]["line"], 5, "{body}");
    assert!(rows[1].get("enclosing").is_none(), "{body}");

    let sweep = format!("fn many() {{\n{}}}\n", "    helper();\n".repeat(60));
    let wide = search_fixture(
        &[("src/b.rs", &sweep)],
        ls_query(serde_json::json!({"matchString": "helper"}), None),
    );
    // A sweep names its owner on every row.
    let rows = wide["files"][0]["matches"].as_array().expect("rows");
    assert_eq!(rows.len(), 60, "{wide}");
    assert!(
        (0..rows.len()).all(|row| owner_of(&wide["files"][0], row)
            == Some(
                serde_json::json!({"symbolName":"many","kind":"function","line":1,"endLine":62})
            )),
        "{wide}"
    );
}
