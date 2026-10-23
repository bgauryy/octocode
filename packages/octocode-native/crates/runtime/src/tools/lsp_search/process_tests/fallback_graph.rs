//! Fallback type-hierarchy graph walks: row and page helpers, partial
//! relations and windowed subtypes.

use super::*;

mod hierarchy;

type ImplementationTargets<'a> = (&'a str, usize, &'a [(&'a str, usize)]);

fn fallback_graph_rows(
    files: &[(&str, &str)],
    definitions: &[(&str, usize, &str, &str, usize)],
    implementations: &[ImplementationTargets<'_>],
    queries: &[(&str, &str, usize, u32)],
) -> Vec<serde_json::Value> {
    fallback_graph_pages(files, definitions, implementations, queries)
        .into_iter()
        .map(|mut pages| {
            assert_eq!(pages.len(), 1, "this fixture expects an unpaged result");
            pages.remove(0)
        })
        .collect()
}

fn fallback_graph_pages(
    files: &[(&str, &str)],
    definitions: &[(&str, usize, &str, &str, usize)],
    implementations: &[ImplementationTargets<'_>],
    queries: &[(&str, &str, usize, u32)],
) -> Vec<Vec<serde_json::Value>> {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::ops::Operation;
    use crate::tools::lsp_search::source::{SourceCache, snippet_policy};
    use serde_json::json;
    let (_dir, root, script) = fixture();
    for (file, content) in files {
        std::fs::write(root.join(file), content).expect("source fixture");
    }
    let declaration = |file: &str, line: usize| {
        let content = files
            .iter()
            .find(|(name, _)| *name == file)
            .expect("file")
            .1;
        let text = content.lines().nth(line).expect("line");
        // A Rust trait impl is located at its self type, after `for`.
        let words = text.split_whitespace().collect::<Vec<_>>();
        let name = match words.iter().position(|word| *word == "for") {
            Some(at) if words[0].starts_with("impl") => words[at + 1],
            _ => words[2],
        };
        let start = text.find(name).expect("name position");
        json!({"uri": octocode_engine::lsp::uri::path_to_uri(&root.join(file).to_string_lossy()).expect("uri"),
            "range": {"start":{"line":line,"character":start},"end":{"line":line,"character":start+name.len()}}})
    };
    let mut graph = json!({"definitions":{},"implementations":{}});
    for (file, line, name, target, target_line) in definitions {
        let text = files
            .iter()
            .find(|(entry, _)| entry == file)
            .expect("file")
            .1
            .lines()
            .nth(*line)
            .expect("line");
        let column = text.rfind(name).expect("reference position");
        graph["definitions"]
            .as_object_mut()
            .expect("definition map")
            .entry(format!("{file}:{line}:{column}"))
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("definition targets")
            .push(declaration(target, *target_line));
    }
    for (file, line, targets) in implementations {
        let node = declaration(file, *line);
        let column = node["range"]["start"]["character"]
            .as_u64()
            .expect("column");
        graph["implementations"][format!("{file}:{line}:{column}")] = json!(
            targets
                .iter()
                .map(|(target, line)| declaration(target, *line))
                .collect::<Vec<_>>()
        );
    }
    let server = FAKE_SERVER
        .replace("hoverProvider: true", "definitionProvider: true, implementationProvider: true")
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            r#"  } else if (msg.method === 'textDocument/definition' || msg.method === 'textDocument/implementation') {
    const graph = JSON.parse(process.env.FAKE_LSP_GRAPH);
    const file = require('path').basename(new URL(msg.params.textDocument.uri).pathname);
    const p = msg.params.position;
    const key = `${file}:${p.line}:${p.character}`;
    const entries = graph[msg.method.endsWith('/definition') ? 'definitions' : 'implementations'];
    send({ jsonrpc: '2.0', id: msg.id, result: entries[key] || [] });"#,
        );
    std::fs::write(&script, server).expect("fixture server");
    let mut config = config(&root, &script, None);
    config.env = Some(
        [("FAKE_LSP_GRAPH".into(), graph.to_string())]
            .into_iter()
            .collect(),
    );
    let paths = crate::tools::test_support::workspace_policy(&root);
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config);
        client.start().await.expect("server starts");
        let mut rows = Vec::new();
        for (operation, file, line, depth) in queries {
            let node = declaration(file, *line);
            let name = files.iter().find(|(entry, _)| entry == file).expect("file").1.lines().nth(*line).expect("line").split_whitespace().nth(2).expect("name");
            let path = root.join(file).to_string_lossy().into_owned();
            let start = json!({"operation":operation,"path":path,"line":line+1,"symbolName":name,"graphDepth":depth});
            let mut pages = Vec::new();
            let mut walker = Walker::new(ToolId::LspSearch.as_str(), json!({"queries":[start]}));
            while let Some(call) = walker.next_query() {
                let wire = call["queries"][0].clone();
                let query: crate::tools::lsp_search::LspSearchQuery = serde_json::from_value(wire.clone()).expect("executable query");
                let mut sources = SourceCache::new(&paths);
                let policy = snippet_policy(&paths);
                let scope = crate::tools::lsp_search::scope::Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
                let row = Operation { client: &client, query: &query, sources: &mut sources, snippet_policy: &policy,
                    cancel: &NeverCancel, path: &path, scope: &scope, root_only: false, line: u32::try_from(*line).expect("line"),
                    character: u32::try_from(node["range"]["start"]["character"].as_u64().expect("character")).expect("character"),
                    language_id:Some("typescript"), budget:16_000, response_window:None }.run().await.expect("fallback hierarchy");
                let row = crate::tools::lsp_search::failure::with_next(&query, row);
                if let Some(next) = row.pointer("/next/nextPage/query/queries/0") {
                    let continued: crate::tools::lsp_search::LspSearchQuery = serde_json::from_value(next.clone()).expect("executable nextPage");
                    assert!(lsp_search_query::page(&continued) > lsp_search_query::page(&query), "page cursor must advance");
                }
                let published = crate::tools::test_support::published(ToolId::LspSearch, &wire, &row);
                walker.accept(Page { output: without_rerun(published, &query), rows: match_rows(&row) });
                pages.push(row);
            }
            walker.finish();
            rows.push(pages);
        }
        client.stop().await.expect("fixture server closed");
        rows
    })
}

#[test]
fn fallback_type_hierarchy_marks_unresolved_relations_partial_without_retry() {
    let rows = fallback_graph_rows(
        &[(
            "unknown.ts",
            "export class Base {}\nexport class Child extends Unknown {}\n",
        )],
        &[],
        &[("unknown.ts", 0, &[("unknown.ts", 1)])],
        &[
            ("supertypes", "unknown.ts", 1, 2),
            ("subtypes", "unknown.ts", 0, 2),
        ],
    );
    for row in rows {
        assert_eq!(row["isPartial"], true, "{row}");
        assert_eq!(
            row["partialReasons"],
            serde_json::json!(["typeHierarchyExpansionFailed"]),
            "{row}"
        );
        // An unresolved base stays unresolved: retrying reruns the same walk.
        assert!(row.pointer("/next/retry").is_none(), "{row}");
        assert!(
            row["matches"].as_array().is_none_or(Vec::is_empty),
            "unverified edges omitted: {row}"
        );
    }
}

#[test]
fn fallback_subtypes_page_the_anchor_candidates_by_window() {
    let cap = crate::tools::lsp_search::walk::MAX_HIERARCHY_NODES;
    let mut content = "export class Base {}\n".to_owned();
    for index in 0..=cap {
        content.push_str(&format!("export class Child{index} extends Base {{}}\n"));
    }
    let definitions = (1..=cap + 1)
        .map(|line| ("wide.ts", line, "Base", "wide.ts", 0))
        .collect::<Vec<_>>();
    let candidates = (1..=cap + 1)
        .map(|line| ("wide.ts", line))
        .collect::<Vec<_>>();
    let mut results = fallback_graph_pages(
        &[("wide.ts", &content)],
        &definitions,
        &[("wide.ts", 0, &candidates)],
        &[("subtypes", "wide.ts", 0, 1)],
    );
    assert_eq!(results.len(), 1);
    let pages = results.remove(0);
    // The anchor's candidates page by window: the first window's rows span
    // result pages, and its last page continues to the next window.
    let first_window = pages
        .iter()
        .take_while(|row| row["partialReasons"] == serde_json::json!(["hierarchyCandidateLimit"]))
        .collect::<Vec<_>>();
    assert!(
        first_window.len() > 1,
        "the verified prefix must cross the page budget"
    );
    for (page_index, row) in first_window.iter().enumerate() {
        assert_eq!(row["pagination"]["totalItems"], cap, "{row}");
        assert_eq!(row["pagination"]["currentPage"], page_index + 1, "{row}");
        assert_eq!(row["isPartial"], true, "{row}");
        assert!(row.get("terminalLimit").is_none(), "{row}");
        assert_eq!(
            row.pointer("/next/nextCandidatePage").is_some(),
            page_index + 1 == first_window.len(),
            "only the window's last page offers the next window: {row}"
        );
        if let Some(snapshot) = row.pointer("/next/nextCandidatePage/query/queries/0/snapshot") {
            assert!(
                snapshot
                    .as_str()
                    .is_some_and(|value| value.starts_with("lsp-candidates:")),
                "candidate continuation pins the complete candidate list: {row}"
            );
        }
    }
    let mut identities = std::collections::HashSet::new();
    let mut reached = 0;
    for row in &pages {
        let shown = row["matches"].as_array().expect("matches");
        assert!(!shown.is_empty(), "each page retains complete rows: {row}");
        for matched in shown {
            assert_eq!(matched["symbolName"], format!("Child{reached}"), "{row}");
            assert_eq!(matched["displayRange"]["startLine"], reached + 2, "{row}");
            assert_eq!(matched["level"], 1, "{row}");
            assert!(
                identities.insert((matched["path"].clone(), matched["displayRange"].to_string())),
                "no repeated declaration: {matched}"
            );
            reached += 1;
        }
    }
    assert_eq!(reached, cap + 1, "every candidate is reachable");
    assert_eq!(
        identities.len(),
        cap + 1,
        "each declaration must be reached once"
    );
    let last = pages.last().expect("final page");
    assert!(last.get("isPartial").is_none(), "{last}");
    // No page remains; the one lead walks one level deeper from the start.
    let next = last["next"].as_object().expect("the deeper-walk lead");
    assert_eq!(
        next.keys().collect::<Vec<_>>(),
        ["walkDeeper"],
        "the route terminates: {last}"
    );
    let deeper = &next["walkDeeper"]["query"]["queries"][0];
    assert_eq!(deeper["graphDepth"], 2, "{last}");
    assert!(
        deeper.get("candidatePage").is_none(),
        "from the start: {last}"
    );
}
