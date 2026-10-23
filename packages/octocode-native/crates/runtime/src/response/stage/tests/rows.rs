use super::*;

/// One envelope past the automatic page pages by rows, and only a paged
/// response copies its envelope for later pages.
#[test]
fn automatic_row_pages_and_only_paged_responses_keep_a_copy() {
    let respond_with = |content: String, scope: Option<PageScope>| {
        let structured = json!({"results":[{"index":0,"data":{"path":"a.txt","content":content,"totalLines":1}}]});
        let mut kept = None;
        let outcome = respond(
            StageInput {
                auto_page_chars: 1_000,
                ..input(
                    ToolId::LocalFetch,
                    structured,
                    ResponsePageOptions {
                        response_length: scope.map(|_| 1_000),
                        response_scope: scope,
                        ..ResponsePageOptions::default()
                    },
                )
            },
            Vec::new(),
            Some(&mut kept),
            &context(),
        )
        .expect("stage runs")
        .expect("valid envelope");
        (outcome, kept.is_some())
    };
    let (outcome, kept) = respond_with("x".repeat(5_000), None);
    assert_eq!(
        outcome.structured_content["responsePagination"]["scope"],
        "rows"
    );
    assert!(kept, "a row-paged response keeps its envelope");
    let (outcome, kept) = respond_with("small".into(), None);
    assert!(
        outcome
            .structured_content
            .get("responsePagination")
            .is_none()
    );
    assert!(!kept, "a one-page response is never copied");
    let (outcome, kept) = respond_with("x".repeat(5_000), Some(PageScope::Structured));
    assert_eq!(
        outcome.structured_content["responsePagination"]["scope"],
        "structuredContent"
    );
    assert!(kept, "a structured window keeps its envelope");
}

/// An explicit `responseLength` alone sizes whole-row pages, never an
/// emptied `results` beside a `responseWindow`, and the continuation
/// keeps the scope it pages by. `responseScope:"structured"` windows
/// the envelope JSON. A response that fits the length is one unpaged
/// response.
#[test]
fn an_explicit_length_pages_rows_and_the_structured_scope_windows_json() {
    let rows: Vec<Value> = (0..4)
        .map(|index| {
            json!({"index":index,"data":{"path":"a.txt","content":"x".repeat(2_000),
                    "totalLines":1}})
        })
        .collect();
    let respond_with = |length: usize, scope: Option<PageScope>| {
        finish(
            input(
                ToolId::LocalFetch,
                json!({ "results": rows }),
                ResponsePageOptions {
                    response_length: Some(length),
                    response_scope: scope,
                    ..ResponsePageOptions::default()
                },
            ),
            &json!({ "queries": vec![json!({"path":"/tmp/a.txt"}); 4] }),
            &context(),
        )
        .expect("stage runs")
        .expect("valid envelope")
    };
    let paged = respond_with(3_000, None).structured_content;
    assert!(
        paged["results"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "{paged}"
    );
    assert!(paged.get("responseWindow").is_none(), "{paged}");
    assert_eq!(paged["responsePagination"]["scope"], "rows", "{paged}");
    let next = ResponsePageOptions::default()
        .follow(&paged["responsePagination"]["next"]["query"])
        .expect("cursor");
    assert_eq!(next.response_scope, Some(PageScope::Rows), "{paged}");
    assert_eq!(next.response_length, Some(3_000), "{paged}");

    let window = respond_with(3_000, Some(PageScope::Structured)).structured_content;
    assert!(window["responseWindow"].is_string(), "{window}");
    assert_eq!(window["results"], json!([]), "{window}");
    assert_eq!(
        ResponsePageOptions::default()
            .follow(&window["responsePagination"]["next"]["query"])
            .and_then(|next| next.response_scope),
        Some(PageScope::Structured),
        "{window}"
    );

    let whole = respond_with(50_000, None);
    assert!(
        whole.structured_content.get("responsePagination").is_none(),
        "{}",
        whole.structured_content
    );
    assert_eq!(
        whole.structured_content["results"].as_array().map(Vec::len),
        Some(4)
    );
}

/// With automatic paging off (or unavailable), a large response is
/// returned complete: no page drops any of its content.
#[test]
fn unpaged_responses_keep_the_complete_content() {
    let content = "x".repeat(64 * 1024);
    let structured =
        json!({"results":[{"index":0,"data":{"path":"a.txt","content":content,"totalLines":1}}]});
    for allow_auto_paging in [true, false] {
        let outcome = finish(
            StageInput {
                auto_page_chars: if allow_auto_paging { 0 } else { 1_000 },
                allow_auto_paging,
                ..input(
                    ToolId::LocalFetch,
                    structured.clone(),
                    ResponsePageOptions::default(),
                )
            },
            &json!({"path":"/tmp/a.txt"}),
            &context(),
        )
        .expect("execution")
        .expect("contract-valid response");
        let page = &outcome.structured_content;
        assert!(page.get("responsePagination").is_none());
        assert_eq!(page["results"][0]["data"]["content"], content);
    }
}
