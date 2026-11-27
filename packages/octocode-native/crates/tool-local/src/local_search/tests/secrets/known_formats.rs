//! A search term runs when a read of the same text hides nothing, and the
//! rows show what a read shows.
use super::*;
use octocode_runtime_base::security::InputPolicy;
use octocode_test_support::json_fixture;
use serde_json::json;

fn gate(term: &str) -> bool {
    ContentSecurity::new()
        .check_input(json!({"matchString": [term]}), InputPolicy::Query)
        .is_ok()
}

#[test]
fn a_term_of_ordinary_code_passes_the_gate_and_finds_its_line() {
    for term in [
        "token::OpenParen",
        "brace_token: token::Brace",
        "password:!0",
        "token:void 0",
        "password: *default_password",
        "password: hunter2",
    ] {
        assert!(gate(term), "{term:?} was refused");
    }
    let source = "if self.token == token::OpenParen {\n    brace_token: token::Brace,\n}\n";
    let rows = search_fixture(
        &[("a.rs", source)],
        ls_query(
            json_fixture(r#"{"matchString": ["token::OpenParen"]}"#),
            None,
        ),
    );
    assert_eq!(rows["matchCount"], 1, "{rows}");
    assert!(!rows.to_string().contains("REDACTED"), "{rows}");
}

#[test]
fn a_term_holding_a_quoted_or_query_secret_is_refused() {
    for term in [
        "password: \"hunter2x!\"",
        "/x?token=abc123xyz&page=2",
        "spring.datasource.password=${MYSQL_PASS:petclinic}",
    ] {
        assert!(!gate(term), "{term:?} was accepted");
    }
}

#[test]
fn a_url_query_value_is_hidden_in_a_row_and_its_neighbours_stay() {
    let rows = search_fixture(
        &[("a.ts", "fetch(\"/x?token=abc123xyz&page=2\");\n")],
        ls_query(json_fixture(r#"{"matchString": ["page=2"]}"#), None),
    );
    assert_eq!(rows["matchCount"], 1, "{rows}");
    let text = rows.to_string();
    assert!(
        !text.contains("abc123xyz") && text.contains("&page=2"),
        "{rows}"
    );
}

#[test]
fn a_paging_token_in_a_row_reads_as_written_and_an_access_token_is_hidden() {
    let rows = search_fixture(
        &[(
            "a.ts",
            "fetch(\"/x?next_token=Abc123Def456&access_token=Zyx987Wvu654\");\n",
        )],
        ls_query(json_fixture(r#"{"matchString": ["fetch"]}"#), None),
    );
    assert_eq!(rows["matchCount"], 1, "{rows}");
    let text = rows.to_string();
    assert!(text.contains("next_token=Abc123Def456"), "{rows}");
    assert!(!text.contains("Zyx987Wvu654"), "{rows}");
}
