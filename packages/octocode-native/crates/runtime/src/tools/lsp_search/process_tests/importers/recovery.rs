//! Importer recovery rows: failed requests, import bindings, symlinked twins,
//! gitignored candidates.

use super::*;
use octocode_contract_types::LocalSearchQuery;

/// A verified importer whose references request fails is a gap in the
/// answer: the row is partial with `importerRequestFailed`, the error, and an
/// optional related reattempt; it never reads as complete.
#[test]
fn a_failed_importer_request_marks_the_row_partial() {
    if !require_node() {
        return;
    }
    let server = IMPORTER_SERVER.replace(
        "      if (uri === declUri) return [decl];\n      return [",
        "      if (uri === declUri) return [decl];\n      if (uri !== declUri) return { __error: 'importer references failed' };\n      return [",
    );
    assert_ne!(server, IMPORTER_SERVER, "the failing branch is injected");
    let (_dir, root, execution) = importer_fixture_serving(&[("f", 2)], &server);
    let pool = fake_server_pool();
    let query_row = serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
    });
    let query = serde_json::from_value(query_row.clone()).expect("query");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let row = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(crate::tools::lsp_search::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
        ))
        .expect("row");
    assert_row_contract(ToolId::LspSearch, &row);
    assert_eq!(row["isPartial"], true, "{row}");
    assert!(
        row["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&serde_json::json!("importerRequestFailed"))),
        "{row}"
    );
    assert!(
        row["warnings"]
            .to_string()
            .contains("importer references failed"),
        "{row}"
    );
    assert!(
        row["next"].get("retry").is_none(),
        "a fresh retry is not mandatory continuation work: {row}"
    );
    assert!(
        row["related"].as_array().is_some_and(|calls| calls
            .iter()
            .any(|call| { call["kind"] == "related" && call["tool"] == "lspSearch" })),
        "a typed transient importer failure offers a same-query related reattempt: {row}"
    );
}

/// `includeDeclaration:false` drops the anchor's declaration, never an
/// importer's import binding: asked from the importer (as tsserver
/// answers), the binding is that file's declaration of its alias, and the
/// anchor's declaration is a reference.
#[test]
fn importer_recovery_without_declarations_keeps_import_bindings() {
    if !require_node() {
        return;
    }
    let honoring = "    case 'textDocument/references': {\n      const uri = p.textDocument.uri;\n      const inc = !!(p.context && p.context.includeDeclaration);\n      if (uri === declUri) return inc ? [decl] : [];\n      return inc ? [{ uri, range: at(0, 9, 6) }, { uri, range: at(1, 0, 6) }, decl] : [{ uri, range: at(1, 0, 6) }, decl];\n    }\n";
    let start = IMPORTER_SERVER
        .find("    case 'textDocument/references': {")
        .expect("references case");
    let end = start
        + IMPORTER_SERVER[start..]
            .find("    case 'textDocument/prepareCallHierarchy':")
            .expect("next case");
    let server = format!(
        "{}{honoring}{}",
        &IMPORTER_SERVER[..start],
        &IMPORTER_SERVER[end..]
    );
    let (_dir, root, execution) = importer_fixture_serving(&[("f", 2)], &server);
    let pool = fake_server_pool();
    let paths = crate::tools::test_support::workspace_policy(&root);
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let rows = |include_declaration: bool| {
        let query_row = serde_json::json!({
            "operation": "references", "path": root.join("a.ts").to_string_lossy(),
            "symbolName": "target", "line": 1, "includeDeclaration": include_declaration,
            "workspaceRoot": root.to_string_lossy()
        });
        let query = serde_json::from_value(query_row.clone()).expect("query");
        let row = runtime
            .block_on(crate::tools::lsp_search::execute(
                query,
                &crate::execution::cancel::NeverCancel,
                &pool,
                &paths,
                &execution,
                None,
            ))
            .expect("row");
        assert_row_contract(ToolId::LspSearch, &row);
        let mut rows = reference_rows(&row);
        rows.sort();
        rows
    };
    let importer_sites = |file: &str| [(file.to_owned(), 1, 10), (file.to_owned(), 2, 1)];
    let mut without = importer_sites("f000.ts").to_vec();
    without.extend(importer_sites("f001.ts"));
    assert_eq!(rows(false), without, "declaration out, bindings in");
    let mut with = vec![("a.ts".to_owned(), 1, 17)];
    with.extend(without);
    assert_eq!(rows(true), with);
}

/// A server that reaches the workspace through a symlink as well (a
/// workspace package linked under `node_modules`) names each site under both
/// spellings, and its `includeDeclaration: false` filter knows the
/// declaration under its real path only. Each site is listed once, under its
/// real path, and the declaration stays out.
#[test]
fn symlinked_twins_list_each_site_once_under_its_real_path() {
    if !require_node() {
        return;
    }
    let twins = "    case 'textDocument/references': {\n      const uri = p.textDocument.uri;\n      const inc = !!(p.context && p.context.includeDeclaration);\n      const linked = (u) => u.replace(root + '/', root + '/node_modules/pkg/');\n      const use = { uri: 'file://' + root + '/f000.ts', range: at(1, 0, 6) };\n      const twins = [use, decl].map((site) => ({ uri: linked(site.uri), range: site.range }));\n      if (uri === declUri) return (inc ? [decl, use] : [use]).concat(twins);\n      return [{ uri, range: at(0, 9, 6) }, use, decl];\n    }\n";
    let start = IMPORTER_SERVER
        .find("    case 'textDocument/references': {")
        .expect("references case");
    let end = start
        + IMPORTER_SERVER[start..]
            .find("    case 'textDocument/prepareCallHierarchy':")
            .expect("next case");
    let server = format!(
        "{}{twins}{}",
        &IMPORTER_SERVER[..start],
        &IMPORTER_SERVER[end..]
    );
    let (_dir, root, execution) = importer_fixture_serving(&[("f", 1)], &server);
    std::fs::create_dir_all(root.join("node_modules")).expect("node_modules");
    std::os::unix::fs::symlink(&root, root.join("node_modules/pkg")).expect("package link");
    let pool = fake_server_pool();
    let paths = crate::tools::test_support::workspace_policy(&root);
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let rows = |include_declaration: bool| {
        let query_row = serde_json::json!({
            "operation": "references", "path": root.join("a.ts").to_string_lossy(),
            "symbolName": "target", "line": 1, "includeDeclaration": include_declaration,
            "workspaceRoot": root.to_string_lossy()
        });
        let query = serde_json::from_value(query_row.clone()).expect("query");
        let row = runtime
            .block_on(crate::tools::lsp_search::execute(
                query,
                &crate::execution::cancel::NeverCancel,
                &pool,
                &paths,
                &execution,
                None,
            ))
            .expect("row");
        assert_row_contract(ToolId::LspSearch, &row);
        let mut sites = Vec::new();
        for site in row["matches"].as_array().expect("full reference locations") {
            let path = Path::new(site["path"].as_str().expect("path"));
            let path = path.strip_prefix(&root).unwrap_or(path);
            sites.push((
                path.to_string_lossy().into_owned(),
                site["displayRange"]["startLine"].as_u64().expect("line"),
            ));
        }
        sites.sort();
        sites
    };
    let site = |path: &str, line: u64| (path.to_owned(), line);
    // Declarations left out first, then in, then out again: the answer does
    // not depend on what the server answered before.
    assert_eq!(
        rows(false),
        [site("f000.ts", 2)],
        "declaration out, one spelling"
    );
    assert_eq!(rows(true), [site("a.ts", 1), site("f000.ts", 2)]);
    assert_eq!(rows(false), [site("f000.ts", 2)]);
}

/// A gitignored file that spells the name is never an importer candidate,
/// and the row says so: partial, with the deciding ignore rule, and a lead
/// whose localSearch rows (ignored files included) reach that file.
#[test]
fn importer_recovery_names_gitignored_files_it_skipped() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = importer_fixture_of(&[("f", 2)]);
    std::fs::create_dir_all(root.join(".git")).expect("repository marker");
    std::fs::write(root.join(".gitignore"), "gen/\n").expect("gitignore");
    std::fs::create_dir_all(root.join("gen")).expect("ignored dir");
    std::fs::write(
        root.join("gen/g000.ts"),
        "import { target } from '../a';\ntarget();\n",
    )
    .expect("ignored importer");
    let pool = fake_server_pool();
    let query_row = serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
    });
    let query = serde_json::from_value(query_row.clone()).expect("query");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let row = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(crate::tools::lsp_search::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
        ))
        .expect("row");
    assert_row_contract(ToolId::LspSearch, &row);
    assert!(
        row["matches"]
            .as_array()
            .is_some_and(|files| files.iter().any(|file| file["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("f001.ts")))),
        "{row}"
    );
    assert!(
        !row["matches"]
            .as_array()
            .is_some_and(|files| files.iter().any(|file| file["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("g000.ts")))),
        "the ignored file is no answer row: {row}"
    );
    assert_eq!(row["isPartial"], true, "{row}");
    assert!(
        row["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&serde_json::json!("importerScanIgnored"))),
        "{row}"
    );
    let warned = row["warnings"].to_string();
    assert!(
        warned.contains("1 imports it") && warned.contains(".gitignore:1 'gen/'"),
        "{row}"
    );
    let lead = &crate::tools::test_support::checks::related_call(&row, "localSearch", |_| true);
    assert_eq!(lead["tool"], "localSearch", "{row}");
    let rows = lead["input"]["queries"].as_array().expect("lead rows");
    let policy = crate::tools::test_support::workspace_policy(&root);
    let found = rows
        .iter()
        .flat_map(|search| {
            assert_eq!(search["includeIgnored"], true, "{search}");
            let query: LocalSearchQuery =
                serde_json::from_value(search.clone()).expect("lead row is a localSearch query");
            crate::tools::local_search::execute_local_search(
                &query,
                &policy,
                &crate::security::ContentSecurity::new(),
                &crate::execution::cancel::NeverCancel,
                None,
                None,
                None,
            )
            .expect("lead runs")
            .files
            .into_iter()
            .map(|file| file.path)
        })
        .collect::<Vec<_>>();
    assert!(
        found.iter().any(|path| path.ends_with("g000.ts")),
        "{found:?}"
    );
}
