use super::*;
use octocode_test_support::json_fixture;

fn item(name: &str) -> ArtifactItem {
    ArtifactItem::new(ArtifactType::Npm, name.to_owned(), String::new())
}

/// The live `["mcp","code search"]` page: npm put a Notion server that
/// matches only `mcp` above packages that spell both terms.
#[test]
fn a_page_lists_packages_matching_more_terms_first() {
    let terms = ["mcp".to_owned(), "code search".to_owned()];
    let packages = [
        json_fixture(
            r#"{
                "name": "@notionhq/notion-mcp-server",
                "description": "Official MCP server for Notion API",
                "keywords": ["notion", "mcp"]
            }"#,
        ),
        json_fixture(
            r#"{
                "name": "smarter-faster-better-mcp",
                "description": "Blazing fast AST-based MCP code search"
            }"#,
        ),
        json_fixture(
            r#"{
                "name": "codeindex-sync",
                "description": "Index sync",
                "keywords": ["mcp", "code-search"]
            }"#,
        ),
        json_fixture(r#"{"name": "unrelated", "description": "nothing here"}"#),
    ];
    let matched = packages
        .iter()
        .map(|package| terms_matched(&terms, Some(package)))
        .collect::<Vec<_>>();
    assert_eq!(matched, [1, 2, 2, 0]);
    let mut page = packages
        .iter()
        .map(|package| item(package["name"].as_str().expect("name")))
        .collect::<Vec<_>>();
    rank_by_terms(&mut page, &matched);
    let names = page
        .iter()
        .map(|row| row.package_name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "smarter-faster-better-mcp",
            "codeindex-sync",
            "@notionhq/notion-mcp-server",
            "unrelated"
        ]
    );
    // A term matches whole words: `mcp` is not in `mcpx`.
    assert_eq!(
        terms_matched(&terms, Some(&json_fixture(r#"{"name": "mcpx"}"#))),
        0
    );
}
