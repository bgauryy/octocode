//! Discovery pages: totals, provider cursors, one-window registries and bounded reach.
use super::*;
use crate::id::ToolId;
use crate::providers::artifact::ArtifactProviderPage;

/// A registry that counts no total (rubygems) states none: the page is
/// built from the shared page facts, so `totalItems` is absent rather
/// than `null`, and the row passes the output contract.
#[test]
fn a_discovery_without_a_registry_total_omits_total_items() {
    let query = typed(&json!({"operation":"search","ecosystem":"rubygems","keywords":["jwt"]}));
    let gem = ArtifactItem::new(
        ArtifactType::Rubygems,
        "jwt".into(),
        "https://rubygems.org/gems/jwt".into(),
    );
    let data = respond(
        &query,
        ArtifactProviderPage {
            artifacts: vec![gem],
            next_state: Some(crate::providers::artifact::ArtifactProviderState {
                offset: Some(1),
                ..Default::default()
            }),
            total: None,
            terminal_limit: None,
            registry: None,
        },
    )
    .expect("rows");
    assert_eq!(
        data["pagination"],
        json!({"page": 1, "pageSize": 20, "hasMore": true}),
        "{data}"
    );
    crate::tools::test_support::validate_sealed(
        ToolId::ArtifactSearch.as_str(),
        &json!({"results": [{"index": 0, "data": data}]}),
    )
    .expect("contract-valid discovery page");
}

/// A token-paged registry's next page carries the cursor the provider
/// issued for it, so the continuation needs no server-side state; the
/// cursor of the page just read is not carried on.
#[test]
fn a_next_page_carries_the_provider_cursor() {
    let query = typed(
        &json!({"operation":"search","ecosystem":"go","keywords":["http"],"page":2,"registryCursor":"page-two"}),
    );
    let row = ArtifactItem::new(
        ArtifactType::Go,
        "example.com/a".into(),
        "https://pkg.go.dev/example.com/a".into(),
    );
    let page = |token: Option<&str>| ArtifactProviderPage {
        artifacts: vec![row.clone()],
        next_state: Some(crate::providers::artifact::ArtifactProviderState {
            page: Some(3),
            token: token.map(str::to_owned),
            ..Default::default()
        }),
        total: Some(9),
        terminal_limit: None,
        registry: None,
    };
    let data = respond(&query, page(Some("page-three"))).expect("rows");
    let next = &data["next"]["nextPage"]["input"]["queries"][0];
    assert_eq!(
        (&next["page"], &next["registryCursor"]),
        (&json!(3), &json!("page-three")),
        "{data}"
    );
    crate::tools::test_support::validate_sealed(
        ToolId::ArtifactSearch.as_str(),
        &json!({"results": [{"index": 0, "data": data}]}),
    )
    .expect("contract-valid cursor page");
    // A registry that pages by number issues no cursor.
    let data = respond(&query, page(None)).expect("rows");
    let next = &data["next"]["nextPage"]["input"]["queries"][0];
    assert_eq!(next["page"], 3, "{data}");
    assert!(next.get("registryCursor").is_none(), "{data}");
}

/// Maven Central answers rows only from its first, so a discovery
/// reaches the rows of one start-0 answer: a page inside them continues
/// even when Central would serve no row at its start, and the page that
/// ends them is the terminal limit, stating the total and the reach.
#[test]
fn maven_discovery_pages_through_its_one_window() {
    let rows = |first: usize| {
        (first..first + 20)
            .map(|n| {
                ArtifactItem::new(
                    ArtifactType::Maven,
                    format!("g:a{n}"),
                    format!("https://central.sonatype.com/artifact/g/a{n}"),
                )
            })
            .collect::<Vec<_>>()
    };
    let page = |page: u64, total: u64| {
        let query = typed(
            &json!({"operation":"search","ecosystem":"maven","keywords":["yaml"],"page":page}),
        );
        let offset = page * 20;
        let data = respond(
            &query,
            ArtifactProviderPage {
                artifacts: rows(offset as usize - 20),
                next_state: (offset < total).then(|| {
                    crate::providers::artifact::ArtifactProviderState {
                        offset: Some(offset),
                        ..Default::default()
                    }
                }),
                total: Some(total),
                terminal_limit: None,
                registry: None,
            },
        )
        .expect("rows");
        let mut placed = json!({"results": [{"index": 0, "data": data.clone()}]});
        crate::response::notices::place(&mut placed, "results");
        crate::tools::test_support::validate_sealed(ToolId::ArtifactSearch.as_str(), &placed)
            .expect("contract-valid maven page");
        data
    };
    // 87 matches: every row is on a page, and nothing is partial.
    let first = page(1, 87);
    assert_eq!(
        first["next"]["nextPage"]["input"]["queries"][0]["page"], 2,
        "{first}"
    );
    assert!(first.get("isPartial").is_none(), "{first}");
    // 500 matches: the reach is the window's 100 rows, through page 5.
    let first = page(1, 500);
    assert_eq!(
        first[crate::response::paging::NOTICES],
        json!(["maven matched 500 results; paging reaches only the first 100 (through page 5)."]),
        "{first}"
    );
    let last = page(5, 500);
    assert_eq!(last["pagination"]["hasMore"], json!(false), "{last}");
    assert!(last["next"].get("nextPage").is_none(), "{last}");
    assert_eq!(last["terminalLimit"], true, "{last}");
    assert_eq!(
        last["warnings"],
        json!([
            "maven matched 500 results; paging reaches only the first 100, which end on this page."
        ]),
        "{last}"
    );
}

/// NuGet search reaches its first 4,000 rows and crates.io its first
/// 1,000: the page that ends them is the terminal limit, stating the
/// total and the reach, through the same rule as every bounded registry,
/// whether the registry pages by offset or by page number.
#[test]
fn bounded_discovery_ends_at_its_reach() {
    for (artifact_type, by_offset, last_page, reach) in [
        (ArtifactType::Nuget, true, 200, 4000),
        (ArtifactType::Crates, false, 50, 1000),
    ] {
        let page = |page: u64| {
            let query = typed(
                &json!({"operation":"search","ecosystem":artifact_type,"keywords":["json"],"page":page}),
            );
            let offset = page * 20;
            let next_state = if by_offset {
                crate::providers::artifact::ArtifactProviderState {
                    offset: Some(offset),
                    ..Default::default()
                }
            } else {
                crate::providers::artifact::ArtifactProviderState {
                    page: Some(page + 1),
                    ..Default::default()
                }
            };
            let data = respond(
                &query,
                ArtifactProviderPage {
                    artifacts: (offset as usize - 20..offset as usize)
                        .map(|n| {
                            ArtifactItem::new(
                                artifact_type,
                                format!("pkg{n}"),
                                format!("https://example.org/pkg{n}"),
                            )
                        })
                        .collect(),
                    next_state: Some(next_state),
                    total: Some(50_000),
                    terminal_limit: None,
                    registry: None,
                },
            )
            .expect("rows");
            let mut placed = json!({"results": [{"index": 0, "data": data.clone()}]});
            crate::response::notices::place(&mut placed, "results");
            crate::tools::test_support::validate_sealed(ToolId::ArtifactSearch.as_str(), &placed)
                .expect("contract-valid bounded page");
            data
        };
        let first = page(1);
        assert_eq!(
            first[crate::response::paging::NOTICES],
            json!([format!(
                "{artifact_type} matched 50000 results; paging reaches only the first {reach} (through page {last_page})."
            )]),
            "{first}"
        );
        let before = page(last_page - 1);
        assert!(before["next"].get("nextPage").is_some(), "{before}");
        let last = page(last_page);
        assert_eq!(last["pagination"]["hasMore"], json!(false), "{last}");
        assert!(last["next"].get("nextPage").is_none(), "{last}");
        assert_eq!(last["terminalLimit"], true, "{last}");
        assert_eq!(
            last["warnings"],
            json!([format!(
                "{artifact_type} matched 50000 results; paging reaches only the first {reach}, which end on this page."
            )]),
            "{last}"
        );
    }
}
