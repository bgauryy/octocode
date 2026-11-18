//! Script-bearing markup (Vue, Svelte, Astro, MDX, HTML): no TypeScript server
//! reads it unaided, so an importer there is a missing use, never a complete
//! answer.

use super::*;

fn references(root: &Path, execution: &crate::lsp_search::LspExecutionConfig) -> serde_json::Value {
    let query = serde_json::from_value(serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
    }))
    .expect("query");
    let pool = fake_server_pool();
    let paths = octocode_runtime_base::test_support::workspace_policy(root);
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let row = runtime
        .block_on(crate::lsp_search::execute(
            query,
            &octocode_runtime_base::execution::cancel::NeverCancel,
            &pool,
            &paths,
            execution,
            None,
            crate::lsp_search::test_answers(),
        ))
        .expect("row");
    runtime.block_on(pool.clear_all());
    assert_row_contract(ToolId::LspSearch, &row);
    row
}

/// A Vue and a Svelte file import the name: the server read neither, so the
/// row says partial, counts them and leads to them.
#[test]
fn markup_importers_keep_the_row_from_reading_complete() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = importer_fixture_of(&[("f", 1)]);
    std::fs::write(
        root.join("App.vue"),
        "<script setup lang=\"ts\">\nimport { target } from \"./a\";\ntarget();\n</script>\n",
    )
    .expect("vue");
    std::fs::write(
        root.join("C.svelte"),
        "<script>import { target as t } from './a'; t();</script>\n",
    )
    .expect("svelte");
    let row = references(&root, &execution);
    assert_eq!(row["isPartial"], true, "{row}");
    assert!(
        row["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&serde_json::json!("importUnresolved"))),
        "{row}"
    );
    assert_eq!(row["coverage"]["state"], "partial", "{row}");
    assert_eq!(row["coverage"]["textOnlyFiles"], 2, "{row}");
    let leads = row["related"].to_string();
    assert!(
        leads.contains("App.vue") && leads.contains("C.svelte"),
        "{row}"
    );
}

/// Markup that holds no use of this declaration changes nothing: prose, an
/// import of a same-named export of another module, a name only in a comment
/// of the template.
#[test]
fn markup_without_a_use_leaves_the_row_as_it_was() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = importer_fixture_of(&[("f", 1)]);
    let shape = |row: &serde_json::Value| {
        let mut row = row.clone();
        // A request's own timing and page identity, not its coverage.
        for field in ["warnings", "next"] {
            row.as_object_mut().expect("row").remove(field);
        }
        row
    };
    let before = shape(&references(&root, &execution));
    assert_eq!(before["coverage"]["state"], "complete", "{before}");
    std::fs::write(root.join("Prose.html"), "<p>target practice</p>\n").expect("html");
    std::fs::write(root.join("Doc.mdx"), "# target\n\nThe target is prose.\n").expect("mdx");
    std::fs::write(
        root.join("Other.vue"),
        "<script setup>\nimport { target } from './other';\ntarget();\n</script>\n",
    )
    .expect("vue");
    assert_eq!(shape(&references(&root, &execution)), before);
}

/// With no module export to bind (a class member), a script or template that
/// reads the member is what the server did not see.
#[test]
fn a_member_read_in_markup_counts_without_an_import() {
    use octocode_engine::signatures::markup::markup_reads;
    for (extension, content, read) in [
        ("vue", "<template>{{ port.target(1) }}</template>\n", true),
        ("svelte", "<script>let target = 1;</script>\n", true),
        ("html", "<p>target practice</p>\n", false),
        ("mdx", "# target\n", false),
    ] {
        assert_eq!(
            markup_reads(content, extension, "target"),
            read,
            "{content}"
        );
    }
}
