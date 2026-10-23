use super::*;

#[test]
fn a_rows_page_covering_everything_keeps_structured_results() {
    let envelope = json!({"results":[{"index":0,"data":{"content":"body"}}]});
    let prepared = page_of(
        "localFetch",
        &envelope,
        &ResponsePageOptions {
            response_scope: Some(PageScope::Rows),
            ..options(1_000)
        },
    );
    assert_eq!(prepared["results"], envelope["results"]);
    // A finished response carries no envelope pagination.
    assert!(prepared.get("responsePagination").is_none(), "{prepared}");
}

#[test]
fn prepares_a_window_with_compact_snapshot_continuation() {
    let window_options = ResponsePageOptions {
        response_scope: Some(PageScope::Structured),
        ..options(8)
    };
    let result = page_of("localFetch", &json!({"results":[]}), &window_options);
    let pagination = &result["responsePagination"];
    let next = follow_only_the_cursor(&window_options, pagination);
    assert_eq!(
        next.response_offset,
        Some(pagination["length"].as_u64().expect("length") as usize)
    );
    assert!(
        pagination["next"]["query"]["responseSnapshot"]
            .as_str()
            .expect("cursor")
            .starts_with("saved-v2:")
    );
}

/// A restart page's continuation is the cursor at the first page of the
/// current response: following it serves page 1, no restart, and it never
/// repeats the input.
#[test]
fn a_restart_continues_at_the_first_page_by_cursor() {
    let envelope = json!({"results":[{"index":0,"data":{"content":"line1\nline2\nline3"}}]});
    let stale = structured_options(8, 8, Some("response-v1:stale".into()));
    let restart = page_of("localFetch", &envelope, &stale);
    crate::contracts::validate_output("localFetch", &restart)
        .expect("continuation must satisfy the public output contract");
    let pagination = &restart["responsePagination"];
    assert_eq!(pagination["restart"], true, "{restart}");
    crate::contracts::validate("localFetch", pagination["next"]["query"].clone())
        .expect("continuation must satisfy the public input contract");
    let first = follow_only_the_cursor(&stale, pagination);
    assert_eq!(first.response_offset, Some(0));
    let page = page_of("localFetch", &envelope, &first);
    assert!(
        page["responsePagination"].get("restart").is_none(),
        "{page}"
    );
    assert_eq!(page["responsePagination"]["currentPage"], 1, "{page}");
}

/// Cold call, then the copied continuation served from a warm cache: the
/// per-row `cache` telemetry must not read as a source change.
#[test]
fn cache_warmth_does_not_restart_a_copied_continuation() {
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
    for scope in [PageScope::Rows, PageScope::Structured] {
        let prepare = |warm: bool, offset: Option<usize>, snapshot: Option<String>| {
            page_of(
                "ghGetFileContent",
                &envelope(warm),
                &ResponsePageOptions {
                    response_length: Some(400),
                    response_offset: offset,
                    response_snapshot: snapshot,
                    response_scope: Some(scope),
                    saved: None,
                },
            )
        };
        let cold = prepare(false, None, None);
        let pagination = &cold["responsePagination"];
        assert_eq!(pagination["hasMore"], true, "{scope:?}: {cold}");
        let next = ResponsePageOptions::default()
            .follow(&pagination["next"]["query"])
            .expect("cursor");
        let warm = prepare(true, next.response_offset, next.response_snapshot);
        let page = &warm["responsePagination"];
        assert!(page.get("restart").is_none(), "{scope:?}: {warm}");
        assert!(page.get("changed").is_none(), "{scope:?}: {warm}");
        assert_eq!(content_of(page), content_of(pagination), "{scope:?}");
        assert!(!warm.to_string().contains("\"cache\""), "{scope:?}: {warm}");
    }
}

/// Two executions of a batch with a GitHub error row differ only in
/// the provider's per-call facts; the copied continuation must page on.
#[test]
fn provider_request_ids_do_not_restart_a_copied_continuation() {
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
    for scope in [PageScope::Rows, PageScope::Structured] {
        let prepare = |call: usize, offset: Option<usize>, snapshot: Option<String>| {
            page_of(
                "ghGetFileContent",
                &envelope(call),
                &ResponsePageOptions {
                    response_length: Some(400),
                    response_offset: offset,
                    response_snapshot: snapshot,
                    response_scope: Some(scope),
                    saved: None,
                },
            )
        };
        let mut offset = None;
        let mut snapshot = None;
        let mut error_rows = Vec::new();
        for call in 1..20 {
            let page = prepare(call * 4096, offset, snapshot.clone());
            let pagination = &page["responsePagination"];
            assert!(
                pagination.get("restart").is_none(),
                "{scope:?} call {call}: {page}"
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
            let next = ResponsePageOptions::default()
                .follow(&pagination["next"]["query"])
                .expect("cursor");
            offset = next.response_offset;
            snapshot = next.response_snapshot;
        }
        if scope == PageScope::Rows {
            // Row pages keep the facts of the execution that served them.
            assert_eq!(error_rows.len(), 1, "{error_rows:?}");
            assert!(error_rows[0]["data"]["requestId"].is_string());
            assert!(error_rows[0]["data"]["rateLimit"]["remaining"].is_number());
        }
    }
}

/// A saved response's later pages cut from one memo built on its first
/// read: each equals the page a fresh prepare of the envelope gives, and no
/// later read reads (copies, re-plans) the envelope again.
#[test]
fn saved_reads_cut_every_page_from_one_memo() {
    let structured = json!({"results": (0..6).map(|index| json!({"index": index, "data": {
        "items": (0..40).map(|line| json!({"line": line, "text": "evidence ".repeat(4)})).collect::<Vec<_>>()
    }})).collect::<Vec<_>>()});
    for scope in [PageScope::Rows, PageScope::Structured] {
        let fresh = walk_pages(&structured, scope, 1_500);
        assert!(fresh.len() > 2, "{scope:?}: the response pages");
        let pages = SavedPages::default();
        let mut options = ResponsePageOptions {
            response_scope: Some(scope),
            ..options(1_500)
        };
        for (index, expected) in fresh.iter().enumerate() {
            // Only the first read has an envelope to build from.
            let envelope = if index == 0 {
                &structured
            } else {
                &Value::Null
            };
            let prepared = prepare_saved(
                &ResponseInput {
                    remaining: Vec::new(),
                    tool: "localSearch".into(),
                    structured: Value::Null,
                    options: options.clone(),
                },
                envelope,
                &pages,
                &CancellationToken::new(),
            )
            .expect("saved page");
            assert_eq!(&prepared, expected, "{scope:?} page {index}");
            let pagination = &prepared["responsePagination"];
            if let Some(next) = options.follow(&pagination["next"]["query"]) {
                options = next;
            }
        }
    }
}

/// Row pages pack whole rows up to the requested length: no page but the
/// last could also have held the row the next page starts with (within the
/// few chars the plan keeps for the widest page numbers), and `totalPages`
/// is the same on every page of the walk.
#[test]
fn row_pages_pack_whole_rows_up_to_the_requested_length() {
    let rows = json!({"root":"/repo","results":(0..60).map(|index| json!({
        "index": index,
        "data": {"repository": format!("owner/repo-{index}"),
            "description": "x".repeat(40 + (index * 37) % 260)}
    })).collect::<Vec<_>>()});
    for length in [1_200, 3_000, 9_000] {
        let pages = walk_pages(&rows, PageScope::Rows, length);
        assert!(pages.len() > 1, "length {length}");
        let total = &pages[0]["responsePagination"]["totalPages"];
        assert_eq!(
            total,
            &json!(pages.len()),
            "length {length}: exact on page 1"
        );
        for (index, pair) in pages.windows(2).enumerate() {
            let page = &pair[0];
            assert_eq!(&page["responsePagination"]["totalPages"], total);
            let rendered = rendered_units(page);
            assert!(
                rendered <= length,
                "length {length}: page {index} {rendered}"
            );
            let next_row = &pair[1]["results"][0];
            let row_chars = octocode_engine::json_size::utf16_units(next_row).unwrap_or(0) + 1;
            assert!(
                rendered + row_chars > length - 24,
                "length {length}: page {index} holds {rendered} chars and left room for the next {row_chars}-char row"
            );
        }
    }
}
