//! The lspSearch lead a declaration hit offers: the same lead whether the
//! search names the declaration by its keyword alone (`fn helper`) or by its
//! whole head (`pub fn helper`), and none where no hit declares the name. A
//! fixture server stands for the file's language server, so no test depends
//! on the servers installed here.
use super::*;

const LIB: &str = "pub fn helper() {}\n// pub fn gone was removed\npub use crate::helper as aliased;\npub fn run() { helper(); }\n";

/// localSearch for `term` over [`LIB`], with leads probing a fixture `.rs`
/// server.
fn search_with_server(term: &str) -> serde_json::Value {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().expect("home");
    let command = home.path().join("fixture-lsp");
    std::fs::write(&command, "#!/bin/sh\nexit 0\n").expect("server");
    std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o755)).expect("executable");
    std::fs::write(
        home.path().join("lsp-servers.json"),
        serde_json::json!({"languageServers":{".rs":{"command":command,"args":[],"languageId":"rust"}}})
            .to_string(),
    )
    .expect("config");
    let options = octocode_engine::lsp::config::LspDiscoveryOptions {
        octocode_home: Some(home.path().to_path_buf()),
        ..Default::default()
    };
    octocode_tools_shared::leads::lsp::with_lead_discovery(options, || {
        search_fixture(
            &[("src/lib.rs", LIB)],
            ls_query(serde_json::json!({"matchString":[term]}), None),
        )
    })
}

/// The lspSearch query of the page's related lead, if it offers one, its
/// `path` (under each fixture's own directory) as the file's name.
fn lsp_lead(body: &serde_json::Value) -> Option<serde_json::Value> {
    octocode_runtime_base::test_support::checks::find_related_call(body, "lspSearch", |_| true).map(
        |call| {
            let mut query = call["input"]["queries"][0].clone();
            let path = query["path"].as_str().expect("lead path");
            assert!(path.ends_with("/src/lib.rs"), "{query}");
            query["path"] = "lib.rs".into();
            query
        },
    )
}

/// A search led by a modifier names the declaration as the keyword search
/// does: the same callers lead at the declaring line.
#[test]
fn a_declaration_head_with_a_modifier_offers_the_keyword_searchs_lead() {
    let keyword = search_with_server("fn helper");
    let lead = lsp_lead(&keyword).unwrap_or_else(|| panic!("keyword lead: {keyword}"));
    assert_eq!(lead["operation"], "callers", "{keyword}");
    assert_eq!(lead["symbolName"], "helper", "{keyword}");
    assert_eq!(lead["line"], 1, "{keyword}");
    let headed = search_with_server("pub fn helper");
    assert_eq!(lsp_lead(&headed), Some(lead), "{headed}");
}

/// No lead where it could not answer: a statement led by a modifier
/// (`pub use`) declares nothing, and a head whose only hit is a comment is
/// no declaration the file's outline names.
#[test]
fn a_head_no_hit_declares_offers_no_lead() {
    for term in ["pub use crate::helper", "pub fn gone"] {
        let body = search_with_server(term);
        assert!(
            body["files"]
                .as_array()
                .is_some_and(|files| !files.is_empty()),
            "{term} hits: {body}"
        );
        assert_eq!(lsp_lead(&body), None, "{term}: {body}");
    }
}

/// A head written up to its signature's opening (`pub fn helper(`, as an
/// agent anchors a declaration) names the same declaration: the same lead.
#[test]
fn a_declaration_head_with_its_signature_opening_offers_the_same_lead() {
    let keyword = search_with_server("fn helper");
    let lead = lsp_lead(&keyword).unwrap_or_else(|| panic!("keyword lead: {keyword}"));
    for term in ["pub fn helper(", "fn helper()"] {
        let body = search_with_server(term);
        assert_eq!(lsp_lead(&body), Some(lead.clone()), "{term}: {body}");
    }
}
