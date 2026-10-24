//! Page cursors: the registry reach, page validation and cursor state.
use super::*;

#[test]
fn constructed_or_stale_cursor_state_is_rejected() {
    for state in [
        ArtifactProviderState {
            offset: Some(MAX_CURSOR_OFFSET + 1),
            ..Default::default()
        },
        ArtifactProviderState {
            page: Some(MAX_CURSOR_PAGE + 1),
            ..Default::default()
        },
        ArtifactProviderState {
            page: Some(0),
            ..Default::default()
        },
    ] {
        let error =
            validate_cursor_state(ArtifactType::Nuget, &state, 50).expect_err("out of range");
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }
    for state in [
        ArtifactProviderState::default(),
        ArtifactProviderState {
            offset: Some(30),
            page: Some(2),
            token: Some("issued".into()),
        },
    ] {
        assert!(validate_cursor_state(ArtifactType::Nuget, &state, 50).is_ok());
    }
}

/// A page-addressed registry's page is bounded by the result position
/// it starts at, like an offset: crates.io serves results below 1,000,
/// so 100-row pages end at page 10 and 30-row pages at page 34.
#[test]
fn a_page_addressed_registry_is_bounded_by_its_page_start() {
    let at = |page: u64| ArtifactProviderState {
        page: Some(page),
        ..Default::default()
    };
    for (page, page_size, admitted) in [
        (10, 100, true),
        (11, 100, false),
        (34, 30, true),
        (35, 30, false),
    ] {
        assert_eq!(
            validate_cursor_state(ArtifactType::Crates, &at(page), page_size).is_ok(),
            admitted,
            "page {page} of {page_size}"
        );
    }
}

/// pkg.go.dev serves its first 1,000 search results only: a Go page
/// starting past them is out of range up front instead of walking
/// tokens toward rows the registry never serves.
#[test]
fn go_pages_end_at_the_registrys_thousandth_result() {
    let at = |page: u64| ArtifactProviderState {
        page: Some(page),
        ..Default::default()
    };
    for (page, page_size, admitted) in [
        (200, 5, true),
        (201, 5, false),
        (1000, 5, false),
        (10, 100, true),
        (11, 100, false),
    ] {
        assert_eq!(
            validate_cursor_state(ArtifactType::Go, &at(page), page_size).is_ok(),
            admitted,
            "page {page} of {page_size}"
        );
    }
}

/// The query's cursor reaches the provider as the page's token, for
/// every paging style; a query without one carries none.
#[test]
fn the_query_cursor_is_the_provider_page_token() {
    for (ecosystem, page) in [("go", Some(3)), ("nuget", None)] {
        let query = query::artifact_query(
            serde_json::json!({"operation":"search","ecosystem": ecosystem, "keywords": ["http"],
                "page": 3, "registryCursor": "issued"}),
            None,
        );
        let state = page_state(&query);
        assert_eq!(state.token.as_deref(), Some("issued"), "{ecosystem}");
        assert_eq!(state.page, page, "{ecosystem}");
    }
    let plain = query::artifact_query(
        serde_json::json!({"operation":"search","ecosystem": "go", "keywords": ["http"]}),
        None,
    );
    assert_eq!(page_state(&plain).token, None);
}

/// The gate reads the contract's capability table, so a new
/// ecosystem level changes one core table, not a native `matches!`.
#[test]
fn version_gate_reads_the_contract_capability_table() {
    assert_eq!(version_support("npm"), "range");
    assert_eq!(version_support("maven"), "exact");
    assert_eq!(version_support("go"), "exact");
    assert_eq!(version_support("packagist"), "none");
    assert_eq!(version_support("unknown"), "none");
    let summary = version_support_summary();
    assert!(
        summary.contains("ranges for crates, npm, pypi"),
        "{summary}"
    );
    assert!(
        summary.contains("exact versions for go, maven, nuget"),
        "{summary}"
    );
}
