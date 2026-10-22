//! Response pager tests.

use super::*;

fn options(length: usize) -> ResponsePageOptions {
    ResponsePageOptions {
        response_length: Some(length),
        ..Default::default()
    }
}

#[test]
fn automatic_paging_accounts_for_both_output_channels() {
    let structured = json!({"results": [{"index": 0, "data": {
        "items": (0..100).map(|line| json!({"line": line, "value": "quoted \"evidence\""})).collect::<Vec<_>>()
    }}]});
    let mut options = ResponsePageOptions::default();
    let text = options.auto_paginate(&structured, 1_000, || {
        panic!("an envelope past the budget pages without drawing its text")
    });
    assert!(text.is_none() && options.rows_scope());
    let mut options = ResponsePageOptions::default();
    let text = options.auto_paginate(&json!({"results": []}), 1_000, || {
        Some(Arc::new("😀".repeat(600)))
    });
    assert!(text.is_none() && options.rows_scope());
    let mut options = ResponsePageOptions::default();
    let text = options.auto_paginate(&json!({"results": []}), 1_000, || {
        Some(Arc::new("fits".into()))
    });
    assert_eq!(text.as_deref().map(String::as_str), Some("fits"));
    assert!(!options.rows_scope());
}

/// An offset or snapshot without a length is not an explicit window: the
/// response keeps the automatic page, so it can never be served unpaged
/// past the contract's `responseLength` maximum.
#[test]
fn an_offset_or_snapshot_alone_keeps_the_automatic_page() {
    let structured = json!({"results": [{"index": 0, "data": {"content": "x".repeat(5_000)}}]});
    for mut options in [
        ResponsePageOptions {
            response_offset: Some(0),
            ..Default::default()
        },
        ResponsePageOptions {
            response_snapshot: Some("response-rows-v1:x".into()),
            ..Default::default()
        },
    ] {
        assert!(!options.explicit());
        assert!(options.auto_paginate(&structured, 1_000, || None).is_none());
        assert!(options.rows_scope());
        assert_eq!(options.response_length, Some(1_000));
    }
    // A length alone pages whole rows at that length, not the automatic
    // page; only a scoped window draws and windows as asked.
    let mut sized = options(3_000);
    assert!(sized.explicit() && !sized.scoped());
    assert!(sized.auto_paginate(&structured, 1_000, || None).is_none());
    assert!(sized.rows_scope());
    assert_eq!(sized.response_length, Some(3_000));
    let mut text = ResponsePageOptions {
        response_scope: Some("text".into()),
        ..options(3_000)
    };
    assert!(text.scoped());
    let drawn = text.auto_paginate(&structured, 1_000, || Some(Arc::new("drawn".into())));
    assert_eq!(
        drawn.as_deref().map(String::as_str),
        Some("drawn"),
        "a text window stays text"
    );
    assert!(!text.rows_scope());
    assert_eq!(text.response_length, Some(3_000));
    // A response that fits the length is not windowed at all.
    let mut fits = options(50_000);
    let drawn = fits.auto_paginate(&structured, 1_000, || Some(Arc::new("drawn".into())));
    assert_eq!(drawn.as_deref().map(String::as_str), Some("drawn"));
    assert_eq!(fits.response_length, None);
}

#[test]
fn a_single_indivisible_row_keeps_its_oversized_disclosure() {
    let content = "whole source evidence ".repeat(300);
    let prepared = ResponsePager::new(ResponsePagerConfig::default())
            .prepare(
                ResponseInput {
                    tool: "localFetch".into(),
                    query: json!({"queries": [{"path": "a.txt", "fullContent": true}]}),
                    structured: json!({"results": [{"index": 0, "data": {"path": "a.txt", "content": content}}]}),
                    rendered_text: None,
                    is_error: false,
                    options: ResponsePageOptions {
                        response_length: Some(1_000),
                        response_scope: Some("rows".into()),
                        ..Default::default()
                    },
                    remaining: Vec::new(),
                },
                &CancellationToken::new(),
            )
            .expect("whole row");
    let output = prepared.structured_content;
    assert_eq!(output["results"][0]["data"]["content"], content);
    let pagination = &output["responsePagination"];
    assert_eq!(pagination["oversized"], true);
    assert_eq!(pagination["hasMore"], false);
    assert!(
        pagination["length"]
            .as_u64()
            .is_some_and(|length| length > 1_000)
    );
    assert!(pagination.get("next").is_none());
}

#[test]
fn unicode_pages_reconstruct_exact_text_and_continuations_are_executable() {
    let text = "a😀b\n🧪研究c😀";
    for length in [1, 2, 3, 7] {
        let mut offset = 0;
        let mut snapshot = None;
        let mut joined = String::new();
        loop {
            let result = paginate_text(
                text,
                &ResponsePageOptions {
                    response_length: Some(length),
                    response_offset: Some(offset),
                    response_snapshot: snapshot.clone(),
                    response_scope: None,
                    render_text: None,
                    saved: None,
                },
            );
            let page = result.pagination.expect("pagination");
            joined.push_str(result.text.split_once('\n').expect("header").1);
            snapshot = Some(page.snapshot.clone());
            if !page.has_more {
                break;
            }
            offset = page.next_char_offset.expect("continuation");
        }
        assert_eq!(joined, text);
    }
}

#[test]
fn snapshot_and_unicode_boundary_mismatches_restart() {
    let first = paginate_text("a😀b", &options(1));
    let snapshot = first.pagination.expect("page").snapshot;
    let split = paginate_text(
        "a😀b",
        &ResponsePageOptions {
            response_length: Some(1),
            response_offset: Some(2),
            response_snapshot: Some(snapshot),
            response_scope: None,
            render_text: None,
            saved: None,
        },
    );
    assert_eq!(split.pagination.expect("page").restart, Some(true));
    let changed = paginate_text(
        "changed",
        &ResponsePageOptions {
            response_length: Some(2),
            response_offset: Some(2),
            response_snapshot: Some("response-v1:stale".into()),
            response_scope: None,
            render_text: None,
            saved: None,
        },
    );
    assert_eq!(changed.pagination.expect("page").changed, Some(true));
}

fn structured_options(
    length: usize,
    offset: usize,
    snapshot: Option<String>,
) -> ResponsePageOptions {
    ResponsePageOptions {
        response_length: Some(length),
        response_offset: Some(offset),
        response_snapshot: snapshot,
        response_scope: Some("structured".into()),
        render_text: None,
        saved: None,
    }
}

#[test]
fn structured_scope_windows_the_envelope_and_pages_reassemble_exactly() {
    let pager = ResponsePager::new(ResponsePagerConfig::default());
    let envelope = json!({"results":[
        {"index":0,"data":{"content":"alpha beta gamma delta epsilon"},"status":"empty"},
        {"index":1,"data":{"content":"zeta eta theta iota kappa lambda"}}
    ]});
    let full = envelope.to_string();
    let mut offset = 0;
    let mut snapshot = None;
    let mut joined = String::new();
    loop {
        let prepared = pager
            .prepare(
                ResponseInput {
                    remaining: Vec::new(),
                    tool: "localFetch".into(),
                    query: json!({"path":"a","mainGoal": "test", "reasoning":"r"}),
                    structured: envelope.clone(),
                    rendered_text: None,
                    is_error: false,
                    options: structured_options(40, offset, snapshot.clone()),
                },
                &CancellationToken::new(),
            )
            .expect("page");
        let out = prepared.structured_content;
        // The windowed envelope stays schema-valid: results present.
        assert_eq!(out["results"], json!([]));
        let window = out["responseWindow"].as_str().expect("window");
        // Window text carries no page header; content mirrors it.
        assert_eq!(prepared.content[0].text, window);
        joined.push_str(window);
        // The final window carries no pagination: nothing remains.
        let Some(pagination) = out.get("responsePagination") else {
            break;
        };
        assert_eq!(pagination["scope"], "structuredContent");
        assert_eq!(pagination["hasMore"], true, "{pagination}");
        snapshot = Some(
            pagination["snapshot"]
                .as_str()
                .expect("snapshot")
                .to_owned(),
        );
        // The continuation is executable and keeps the opt-in scope.
        let next = &pagination["next"]["query"];
        assert_eq!(next["responseScope"], "structured");
        offset = pagination["nextOffset"].as_u64().expect("offset") as usize;
    }
    assert_eq!(
        joined, full,
        "concatenated windows must equal the envelope JSON"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&joined).expect("json"),
        envelope
    );
}

#[test]
fn structured_scope_restart_keeps_the_window_empty() {
    let pager = ResponsePager::new(ResponsePagerConfig::default());
    let prepared = pager
        .prepare(
            ResponseInput {
                remaining: Vec::new(),
                tool: "localFetch".into(),
                query: json!({"path":"a","mainGoal": "test", "reasoning":"r"}),
                structured: json!({"results":[{"index":0,"data":{"content":"body"}}]}),
                rendered_text: None,
                is_error: false,
                options: structured_options(10, 5, Some("response-v1:stale".into())),
            },
            &CancellationToken::new(),
        )
        .expect("restart page");
    let out = prepared.structured_content;
    assert_eq!(out["responsePagination"]["restart"], json!(true));
    assert_eq!(
        out["responseWindow"], "",
        "restart pages carry no envelope bytes"
    );
}

/// Pages one response in `scope` at `length`, following each page's own
/// `nextOffset` and `snapshot` exactly as a caller copies them, and returns
/// every page's `(structuredContent, text)`.
fn walk_pages(
    structured: &Value,
    rendered_text: Option<&str>,
    scope: &str,
    length: usize,
) -> Vec<(Value, String)> {
    let pager = ResponsePager::new(ResponsePagerConfig::default());
    let mut options = ResponsePageOptions {
        response_scope: Some(scope.into()),
        ..options(length)
    };
    let mut pages = Vec::new();
    loop {
        let prepared = pager
            .prepare(
                ResponseInput {
                    remaining: Vec::new(),
                    tool: "localSearch".into(),
                    query: json!({"path":"a","matchString":"m"}),
                    structured: structured.clone(),
                    rendered_text: rendered_text.map(|text| Arc::new(text.to_owned())),
                    is_error: false,
                    options: options.clone(),
                },
                &CancellationToken::new(),
            )
            .expect("page");
        let pagination = prepared.structured_content["responsePagination"].clone();
        pages.push((
            prepared.structured_content,
            prepared.content[0].text.clone(),
        ));
        if pagination["hasMore"] != true {
            break;
        }
        assert!(
            pages.len() <= 20_000,
            "{scope} length {length}: the walk must terminate"
        );
        options.response_offset = pagination["nextOffset"].as_u64().map(|v| v as usize);
        options.response_snapshot = pagination["snapshot"].as_str().map(str::to_owned);
    }
    pages
}

/// Whole-response paging delivers every byte or row exactly once at every
/// page size, including sizes smaller than one code point or one row.
/// `text` and `structured` windows concatenate to the exact source;
/// `rows` pages are complete JSON envelopes that together carry each match
/// once. A page's body stays inside its length unless it is one
/// indivisible code point (`text`, `structured`) or fragment (`rows`).
#[test]
fn every_scope_delivers_each_byte_and_row_exactly_once_at_small_sizes() {
    let text = "a😀b\n🧪研究c\\n😀 tail\nlast line é";
    let envelope = json!({"results":[
        {"index":0,"data":{"content":"alpha 😀 beta\ngamma 研究 delta"}},
        {"index":1,"data":{"content":"zeta é eta"}}
    ]});
    let serialized = envelope.to_string();
    for length in (1..=40).chain([64, 100, 257]) {
        let pages = walk_pages(&envelope, Some(text), "text", length);
        let joined: String = pages
            .iter()
            .map(|(_, page)| page.split_once('\n').expect("page header").1)
            .collect();
        assert_eq!(joined, text, "text length {length}");
        for (structured, page) in &pages {
            // One page that covers everything keeps its rows and adds no window.
            if pages.len() > 1 {
                let window = structured["responseWindow"].as_str().expect("window");
                assert_eq!(window, page, "text length {length}: both channels");
            }
            // The last page finishes the response and carries no pagination.
            if let Some(width) = structured["responsePagination"]["length"].as_u64() {
                assert!(
                    width as usize <= length.max(2),
                    "text length {length}: {width} units"
                );
            }
        }

        let pages = walk_pages(&envelope, None, "structured", length);
        let joined: String = pages
            .iter()
            .map(|(structured, _)| structured["responseWindow"].as_str().expect("window"))
            .collect();
        assert_eq!(joined, serialized, "structured length {length}");
        for (structured, page) in &pages {
            assert_eq!(structured["responseWindow"], json!(page));
        }
    }

    const MATCHES: u64 = 12;
    let matches = |file: &str| {
        (0..MATCHES)
            .map(|line| json!({"line": line, "value": format!("{file} item {line} 😀 é")}))
            .collect::<Vec<_>>()
    };
    let rows = json!({"root":"/repo","results":[
        {"index":0,"data":{"files":[{"path":"a.rs","matches":matches("a")}]}},
        {"index":1,"data":{"content":"small"}},
        {"index":2,"data":{"files":[{"path":"c.rs","matches":matches("c")}]}}
    ]});
    let mut expected: Vec<(u64, String, u64)> = (0..MATCHES)
        .flat_map(|line| [(0, "a.rs".to_owned(), line), (2, "c.rs".to_owned(), line)])
        .collect();
    expected.sort();
    for length in [1, 26, 64, 100, 257, 1_000, 50_000] {
        let pages = walk_pages(&rows, None, "rows", length);
        let mut seen = Vec::new();
        let mut small = 0;
        for (structured, text) in &pages {
            let page: Value = serde_json::from_str(text).expect("a page is complete JSON");
            assert_eq!(&page, structured, "rows length {length}: both channels");
            let pagination = &structured["responsePagination"];
            // The whole rendered page, pagination block included, fits.
            let rendered = text.encode_utf16().count();
            assert!(
                rendered <= length || pagination["oversized"] == true,
                "rows length {length}: {rendered} chars unflagged"
            );
            for row in structured["results"].as_array().expect("rows") {
                small += usize::from(row["data"]["content"] == "small");
                let index = row["index"].as_u64().expect("index");
                for file in row["data"]["files"].as_array().into_iter().flatten() {
                    for item in file["matches"].as_array().expect("matches") {
                        seen.push((
                            index,
                            file["path"].as_str().expect("path").to_owned(),
                            item["line"].as_u64().expect("line"),
                        ));
                    }
                }
            }
        }
        seen.sort();
        assert_eq!(seen, expected, "rows length {length}");
        assert_eq!(small, 1, "rows length {length}: the small row appears once");
    }
}

/// A row page plus its `responsePagination` block is at most the
/// requested `responseLength`, at the lengths callers use, for a response
/// of many rows and for one broad row split into parts; pages still
/// deliver every match exactly once.
/// `rows` small rows and one broad row of `broad` matches, at `lengths`.
fn row_pages_fit_the_requested_length(rows: usize, broad: u64, lengths: &[usize]) {
    let matches = |file: &str, count: u64| {
        (0..count)
            .map(|line| json!({"line": line, "value": format!("{file} item {line} 😀 é")}))
            .collect::<Vec<_>>()
    };
    let responses = [
        // Many small rows.
        json!({"root":"/repo","results":(0..rows).map(|index| json!({
                "index": index,
                "data": {"files":[{"path":format!("f{index}.rs"),"matches":matches("m", 3)}]}
            })).collect::<Vec<_>>()}),
        // One broad row.
        json!({"root":"/repo","results":[
            {"index":0,"data":{"files":[{"path":"a.rs","matches":matches("a", broad)}]}}
        ]}),
    ];
    for (shape, response) in responses.iter().enumerate() {
        let rendered = response.to_string().encode_utf16().count();
        for &length in lengths {
            let pages = walk_pages(response, None, "rows", length);
            assert_eq!(
                pages.len() > 1,
                rendered > length,
                "shape {shape} length {length}"
            );
            let mut seen = 0;
            for (structured, text) in &pages {
                let page = text.encode_utf16().count();
                assert!(
                    page <= length || structured["responsePagination"]["oversized"] == true,
                    "shape {shape} length {length}: a {page}-char page"
                );
                for row in structured["results"].as_array().expect("rows") {
                    for file in row["data"]["files"].as_array().expect("files") {
                        seen += file["matches"].as_array().expect("matches").len();
                    }
                }
            }
            let expected: usize = response["results"]
                .as_array()
                .expect("rows")
                .iter()
                .map(|row| {
                    row["data"]["files"][0]["matches"]
                        .as_array()
                        .expect("m")
                        .len()
                })
                .sum();
            assert_eq!(seen, expected, "shape {shape} length {length}");
        }
    }
}

/// The default run walks a smaller response at two lengths; the full
/// matrix (ignored: `cargo test -- --ignored full_matrix`) walks the
/// sizes callers reach at every length.
#[test]
fn a_row_page_with_its_pagination_block_fits_the_requested_length() {
    row_pages_fit_the_requested_length(300, 600, &[4_000, 20_000]);
}

#[test]
#[ignore = "full matrix: cargo test -- --ignored full_matrix"]
fn a_row_page_with_its_pagination_block_fits_the_requested_length_full_matrix() {
    row_pages_fit_the_requested_length(1_500, 3_000, &[4_000, 20_000, 50_000]);
}

#[test]
fn default_scope_is_unchanged_by_the_new_field() {
    let pager = ResponsePager::new(ResponsePagerConfig::default());
    let envelope = json!({"results":[{"index":0,"data":{"content":"body"}}]});
    let prepared = pager
        .prepare(
            ResponseInput {
                remaining: Vec::new(),
                tool: "localFetch".into(),
                query: json!({"path":"a","mainGoal": "test", "reasoning":"r"}),
                structured: envelope.clone(),
                rendered_text: None,
                is_error: false,
                options: ResponsePageOptions::default(),
            },
            &CancellationToken::new(),
        )
        .expect("default");
    assert_eq!(prepared.structured_content, envelope);
    assert!(prepared.structured_content.get("responseWindow").is_none());
}

#[test]
fn a_text_page_covering_everything_keeps_structured_results() {
    let pager = ResponsePager::new(ResponsePagerConfig::default());
    let envelope = json!({"results":[{"index":0,"data":{"content":"body"}}]});
    let prepared = pager
        .prepare(
            ResponseInput {
                remaining: Vec::new(),
                tool: "localFetch".into(),
                query: json!({"path":"a","mainGoal": "test", "reasoning":"r"}),
                structured: envelope.clone(),
                rendered_text: Some(Arc::new("body".into())),
                is_error: false,
                options: ResponsePageOptions {
                    response_length: Some(1_000),
                    ..Default::default()
                },
            },
            &CancellationToken::new(),
        )
        .expect("page");
    assert_eq!(prepared.structured_content["results"], envelope["results"]);
    // A finished response carries no envelope pagination.
    assert!(
        prepared
            .structured_content
            .get("responsePagination")
            .is_none(),
        "{}",
        prepared.structured_content
    );
}

/// MCP hosts that surface only structuredContent must still receive the
/// page: a split text page carries its window in `responseWindow`.
#[test]
fn a_split_text_page_carries_its_window_in_structured_content() {
    let pager = ResponsePager::new(ResponsePagerConfig::default());
    let envelope = json!({"results":[{"index":0,"data":{"content":"body"}}]});
    let text = "line1\nline2\nline3\nline4\n";
    let mut options = options(12);
    let mut joined = String::new();
    loop {
        let prepared = pager
            .prepare(
                ResponseInput {
                    remaining: Vec::new(),
                    tool: "localFetch".into(),
                    query: json!({"path":"a","mainGoal": "test", "reasoning":"r"}),
                    structured: envelope.clone(),
                    rendered_text: Some(Arc::new(text.into())),
                    is_error: false,
                    options: options.clone(),
                },
                &CancellationToken::new(),
            )
            .expect("page");
        let structured = &prepared.structured_content;
        assert_eq!(structured["results"], json!([]));
        let window = structured["responseWindow"].as_str().expect("window");
        assert_eq!(window, prepared.content[0].text);
        joined.push_str(window.split_once('\n').map_or("", |(_, body)| body));
        let pagination = &structured["responsePagination"];
        if pagination["hasMore"] != true {
            break;
        }
        options.response_offset = pagination["nextOffset"].as_u64().map(|v| v as usize);
        options.response_snapshot = pagination["snapshot"].as_str().map(str::to_owned);
    }
    assert_eq!(joined, text);
}

#[test]
fn prepares_both_channels_with_compact_snapshot_continuation() {
    let pager = ResponsePager::new(ResponsePagerConfig::default());
    let result = pager
        .prepare(
            ResponseInput {
                remaining: Vec::new(),
                tool: "localFetch".into(),
                query: json!({"path":"a", "mainGoal":"g", "reasoning":"r", "extras":["stats"]}),
                structured: json!({"results":[]}),
                rendered_text: Some(Arc::new("line1\nline2\nline3".into())),
                is_error: false,
                options: options(8),
            },
            &CancellationToken::new(),
        )
        .expect("page");
    assert_eq!(result.content.len(), 1);
    let next = &result.structured_content["responsePagination"]["next"]["query"];
    assert!(
        next.get("queries").is_none(),
        "saved reads do not repeat input"
    );
    assert!(
        next["responseSnapshot"]
            .as_str()
            .expect("snapshot")
            .starts_with("response-v1:")
    );
}

#[test]
fn continuations_preserve_queries_only_when_starting_a_new_execution() {
    let pager = ResponsePager::new(ResponsePagerConfig::default());
    let queries = json!([
        {"path":"/repo/a.ts", "mainGoal": "test", "reasoning":"read both", "extras":["stats","receipts"], "fullContent":true},
        {"path":"/repo/b.ts", "mainGoal": "test", "reasoning":"read both", "ranges":["2-5"]}
    ]);
    for options in [
        options(8),
        ResponsePageOptions {
            response_length: Some(8),
            response_offset: Some(8),
            response_snapshot: Some("response-v1:stale".into()),
            response_scope: None,
            render_text: None,
            saved: None,
        },
    ] {
        let restarting = options.response_offset.is_some();
        let result = pager
            .prepare(
                ResponseInput {
                    remaining: Vec::new(),
                    tool: "localFetch".into(),
                    query: json!({"queries":queries}),
                    structured: json!({"results":[]}),
                    rendered_text: Some(Arc::new("line1\nline2\nline3".into())),
                    is_error: false,
                    options,
                },
                &CancellationToken::new(),
            )
            .expect("page");
        crate::contracts::validate_output("localFetch", &result.structured_content)
            .expect("continuation must satisfy the public output contract");
        let next = &result.structured_content["responsePagination"]["next"]["query"];
        crate::contracts::validate("localFetch", next.clone())
            .expect("continuation must satisfy the public input contract");
        if restarting {
            assert_eq!(next["queries"], queries);
            let prepared = crate::contracts::prepare_many_and_validate("localFetch", next.clone())
                .expect("restart is a new tool call");
            assert_eq!(prepared.len(), 2);
            for (prepared, original) in prepared.iter().zip(queries.as_array().expect("queries")) {
                for (field, value) in original.as_object().expect("query") {
                    assert_eq!(&prepared[field], value);
                }
            }
        } else {
            assert!(next.get("queries").is_none());
        }
        assert_eq!(next["responseLength"], 8);
        assert_eq!(next["responseSnapshot"].is_null(), restarting);
        if restarting {
            assert_eq!(next["responseOffset"], 0);
        }
    }
}

#[test]
fn frozen_line_boundary_empty_and_last_page_metadata_match() {
    let line = paginate_text("0123456789\n0123456789abc", &options(15));
    assert_eq!(
        line.text.split_once('\n').expect("header").1,
        "0123456789\n"
    );
    assert_eq!(line.pagination.expect("page").next_char_offset, Some(11));

    let empty = paginate_text("", &options(100));
    let empty_page = empty.pagination.expect("page");
    assert_eq!(empty_page.total_chars, 0);
    assert!(!empty_page.has_more);
    assert_eq!(empty_page.next_char_offset, None);

    let short = paginate_text("short", &options(100));
    assert!(!short.pagination.expect("page").has_more);
}

#[test]
fn missing_snapshot_and_changed_beyond_end_return_typed_restart() {
    let missing = paginate_text(
        "a".repeat(100).as_str(),
        &ResponsePageOptions {
            response_length: Some(10),
            response_offset: Some(10),
            ..Default::default()
        },
    );
    let missing_page = missing.pagination.expect("page");
    assert_eq!(missing_page.restart, Some(true));
    assert_eq!(missing_page.changed, Some(false));
    assert_eq!(missing_page.next_char_offset, Some(0));

    let stale = paginate_text(
        "short",
        &ResponsePageOptions {
            response_length: Some(5),
            response_offset: Some(500),
            response_snapshot: Some("response-v1:stale".into()),
            response_scope: None,
            render_text: None,
            saved: None,
        },
    );
    let stale_page = stale.pagination.expect("page");
    assert_eq!(stale_page.char_offset, 5);
    assert_eq!(stale_page.char_length, 0);
    assert_eq!(stale_page.changed, Some(true));
}

#[test]
fn cancellation_fails_before_cache_growth() {
    let pager = ResponsePager::new(ResponsePagerConfig {
        max_rendered_bytes: 4,
    });
    let input = ResponseInput {
        remaining: Vec::new(),
        tool: "tool".into(),
        query: json!({}),
        structured: json!({}),
        rendered_text: Some(Arc::new("large".into())),
        is_error: false,
        options: options(2),
    };
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        pager.prepare(input, &cancelled),
        Err(ResponseError::Cancelled)
    ));
}

/// Rendered text past the byte ceiling is served as row pages with an
/// executable continuation, never a whole-call failure; walking the pages
/// returns every row exactly once.
#[test]
fn oversized_rendered_text_falls_back_to_row_pages() {
    let pager = ResponsePager::new(ResponsePagerConfig {
        max_rendered_bytes: 600,
    });
    let rows: Vec<Value> = (0..6)
            .map(|index| json!({"index": index, "data": {"content": format!("{index}:{}", "x".repeat(200))}}))
            .collect();
    let structured = json!({"results": rows});
    let text = "y".repeat(2_000);
    let mut request = ResponsePageOptions::default();
    let mut seen = Vec::new();
    for _ in 0..20 {
        let prepared = pager
            .prepare(
                ResponseInput {
                    remaining: Vec::new(),
                    tool: "localFetch".into(),
                    query: json!({"queries": [{"path": "a"}]}),
                    structured: structured.clone(),
                    rendered_text: Some(Arc::new(text.clone())),
                    is_error: false,
                    options: request.clone(),
                },
                &CancellationToken::new(),
            )
            .expect("an oversized response pages instead of failing");
        let page = prepared.structured_content;
        for row in page["results"].as_array().expect("rows") {
            seen.push(row["index"].as_u64().expect("index"));
        }
        let text_len = prepared.content.iter().map(|c| c.text.len()).sum::<usize>();
        assert!(text_len <= 2 * 600, "page text stays bounded: {text_len}");
        let Some(next) = page["responsePagination"]["next"]["query"].as_object() else {
            break;
        };
        assert_eq!(next["responseScope"], "rows", "{next:?}");
        request = serde_json::from_value(Value::Object(next.clone())).expect("options");
    }
    assert_eq!(seen, vec![0, 1, 2, 3, 4, 5]);
}

/// One row too large for any row page falls back to envelope
/// windows that concatenate back to the exact envelope JSON.
#[test]
fn oversized_single_row_falls_back_to_concatenable_windows() {
    let pager = ResponsePager::new(ResponsePagerConfig {
        max_rendered_bytes: 600,
    });
    let structured = json!({"results": [{"index": 0, "data": {"content": "z".repeat(2_000)}}]});
    let mut request = ResponsePageOptions::default();
    let mut joined = String::new();
    for _ in 0..50 {
        let prepared = pager
            .prepare(
                ResponseInput {
                    remaining: Vec::new(),
                    tool: "localFetch".into(),
                    query: json!({"queries": [{"path": "a"}]}),
                    structured: structured.clone(),
                    rendered_text: Some(Arc::new("y".repeat(2_000))),
                    is_error: false,
                    options: request.clone(),
                },
                &CancellationToken::new(),
            )
            .expect("pages");
        let page = prepared.structured_content;
        joined.push_str(page["responseWindow"].as_str().expect("window"));
        let Some(next) = page["responsePagination"]["next"]["query"].as_object() else {
            break;
        };
        assert_eq!(next["responseScope"], "structured", "{next:?}");
        request = serde_json::from_value(Value::Object(next.clone())).expect("options");
    }
    assert_eq!(
        serde_json::from_str::<Value>(&joined).expect("windows join"),
        structured
    );
}

/// Cold call, then the copied continuation served from a warm cache: the
/// per-row `cache` telemetry must not read as a source change.
#[test]
fn cache_warmth_does_not_restart_a_copied_continuation() {
    let pager = ResponsePager::new(ResponsePagerConfig::default());
    let envelope = |warm: bool| {
        let rows = (0..3)
            .map(|index| {
                let mut row = json!({"index":index,"data":{"content":"x".repeat(300)}});
                if warm {
                    row["cache"] = json!(1);
                }
                row
            })
            .collect::<Vec<_>>();
        json!({"shared":{"commitSha":"abc"},"results":rows})
    };
    for scope in ["rows", "structured"] {
        let prepare = |warm: bool, offset: Option<usize>, snapshot: Option<String>| {
            pager
                .prepare(
                    ResponseInput {
                        remaining: Vec::new(),
                        tool: "ghGetFileContent".into(),
                        query: json!({"queries":[{"owner":"o","repo":"r","path":"a"}]}),
                        structured: envelope(warm),
                        rendered_text: None,
                        is_error: false,
                        options: ResponsePageOptions {
                            response_length: Some(400),
                            response_offset: offset,
                            response_snapshot: snapshot,
                            response_scope: Some(scope.into()),
                            render_text: None,
                            saved: None,
                        },
                    },
                    &CancellationToken::new(),
                )
                .expect("page")
                .structured_content
        };
        let cold = prepare(false, None, None);
        let pagination = &cold["responsePagination"];
        assert_eq!(pagination["hasMore"], true, "{scope}: {cold}");
        let next = pagination["next"]["query"].clone();
        let warm = prepare(
            true,
            next["responseOffset"].as_u64().map(|x| x as usize),
            next["responseSnapshot"].as_str().map(str::to_owned),
        );
        let page = &warm["responsePagination"];
        assert!(page.get("restart").is_none(), "{scope}: {warm}");
        assert!(page.get("changed").is_none(), "{scope}: {warm}");
        assert_eq!(page["snapshot"], pagination["snapshot"], "{scope}");
        assert!(!warm.to_string().contains("\"cache\""), "{scope}: {warm}");
    }
}

/// Two executions of a batch with a GitHub error row differ only in
/// the provider's per-call facts; the copied continuation must page on.
#[test]
fn provider_request_ids_do_not_restart_a_copied_continuation() {
    let pager = ResponsePager::new(ResponsePagerConfig::default());
    let envelope = |call: usize| {
        let mut rows = (0..3)
            .map(|index| json!({"index":index,"data":{"content":"x".repeat(300)}}))
            .collect::<Vec<_>>();
        rows.push(json!({"index":3,"status":"error","data":{
                "error":"Repository, resource, or path not found","errorCode":"notFound",
                "httpStatus":404,
                "requestId":format!("{call:X}F4:376635:800A3C:A22BF9:6ABD2D60"),
                "rateLimit":{"remaining":100_000 + call,"resetEpochSeconds":1_700_000_000 + call,"retryAfterSeconds":call},
                "retryAfterSeconds":call}}));
        json!({"results":rows})
    };
    for scope in ["rows", "structured"] {
        let prepare = |call: usize, offset: Option<usize>, snapshot: Option<String>| {
            pager
                .prepare(
                    ResponseInput {
                        remaining: Vec::new(),
                        tool: "ghGetFileContent".into(),
                        query: json!({"queries":[{"owner":"o","repo":"r","path":"a"}]}),
                        structured: envelope(call),
                        rendered_text: None,
                        is_error: false,
                        options: ResponsePageOptions {
                            response_length: Some(400),
                            response_offset: offset,
                            response_snapshot: snapshot,
                            response_scope: Some(scope.into()),
                            render_text: None,
                            saved: None,
                        },
                    },
                    &CancellationToken::new(),
                )
                .expect("page")
                .structured_content
        };
        let mut offset = None;
        let mut snapshot = None;
        let mut error_rows = Vec::new();
        for call in 1..20 {
            let page = prepare(call * 4096, offset, snapshot.clone());
            let pagination = &page["responsePagination"];
            assert!(
                pagination.get("restart").is_none(),
                "{scope} call {call}: {page}"
            );
            error_rows.extend(
                page["results"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|row| row["status"] == "error")
                    .cloned(),
            );
            if pagination["hasMore"] != true {
                break;
            }
            let next = &pagination["next"]["query"];
            offset = next["responseOffset"].as_u64().map(|x| x as usize);
            snapshot = next["responseSnapshot"].as_str().map(str::to_owned);
        }
        if scope == "rows" {
            // Row pages keep the facts of the execution that served them.
            assert_eq!(error_rows.len(), 1, "{error_rows:?}");
            assert!(error_rows[0]["data"]["requestId"].is_string());
            assert!(error_rows[0]["data"]["rateLimit"]["remaining"].is_number());
        }
    }
}

/// A saved response's later pages cut from one memo built on its first
/// read: each equals the page a fresh prepare of the envelope gives, and no
/// later read draws the text or reads (copies, re-plans) the envelope again.
#[test]
fn saved_reads_cut_every_page_from_one_memo() {
    let structured = json!({"results": (0..6).map(|index| json!({"index": index, "data": {
        "items": (0..40).map(|line| json!({"line": line, "text": "evidence ".repeat(4)})).collect::<Vec<_>>()
    }})).collect::<Vec<_>>()});
    let text = (0..400)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    let pager = ResponsePager::new(ResponsePagerConfig::default());
    for scope in ["rows", "structured", "text"] {
        let fresh = walk_pages(
            &structured,
            (scope == "text").then_some(text.as_str()),
            scope,
            1_500,
        );
        assert!(fresh.len() > 2, "{scope}: the response pages");
        let pages = SavedPages::default();
        let drawn = std::cell::Cell::new(0);
        let mut options = ResponsePageOptions {
            response_scope: Some(scope.into()),
            ..options(1_500)
        };
        for (index, (expected, expected_text)) in fresh.iter().enumerate() {
            // Only the first read has an envelope to build from.
            let envelope = if index == 0 {
                &structured
            } else {
                &Value::Null
            };
            let prepared = pager
                .prepare_saved(
                    &ResponseInput {
                        remaining: Vec::new(),
                        tool: "localSearch".into(),
                        query: json!({"path":"a","matchString":"m"}),
                        structured: Value::Null,
                        rendered_text: None,
                        is_error: false,
                        options: options.clone(),
                    },
                    envelope,
                    &pages,
                    |_| {
                        drawn.set(drawn.get() + 1);
                        text.clone()
                    },
                    &CancellationToken::new(),
                )
                .expect("saved page");
            assert_eq!(
                &prepared.structured_content, expected,
                "{scope} page {index}"
            );
            assert_eq!(
                &prepared.content[0].text, expected_text,
                "{scope} page {index}"
            );
            let pagination = &prepared.structured_content["responsePagination"];
            options.response_offset = pagination["nextOffset"].as_u64().map(|v| v as usize);
            options.response_snapshot = pagination["snapshot"].as_str().map(str::to_owned);
        }
        assert_eq!(drawn.get(), usize::from(scope == "text"), "{scope}");
    }
}
