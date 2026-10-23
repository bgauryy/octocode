use super::*;

#[test]
fn structured_scope_windows_the_envelope_and_pages_reassemble_exactly() {
    let envelope = json!({"results":[
        {"index":0,"data":{"content":"alpha beta gamma delta epsilon"},"status":"empty"},
        {"index":1,"data":{"content":"zeta eta theta iota kappa lambda"}}
    ]});
    let full = envelope.to_string();
    let mut options = structured_options(40, 0, None);
    let mut joined = String::new();
    loop {
        let out = page_of("localFetch", &envelope, &options);
        // The windowed envelope stays schema-valid: results present.
        assert_eq!(out["results"], json!([]));
        let window = out["responseWindow"].as_str().expect("window");
        joined.push_str(window);
        // The final window carries no pagination: nothing remains.
        let Some(pagination) = out.get("responsePagination") else {
            break;
        };
        assert_eq!(pagination["scope"], "structuredContent");
        assert_eq!(pagination["hasMore"], true, "{pagination}");
        // The continuation is the cursor alone, and keeps the opt-in scope.
        options = follow_only_the_cursor(&options, pagination);
        assert_eq!(options.response_scope, Some(PageScope::Structured));
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
    let out = page_of(
        "localFetch",
        &json!({"results":[{"index":0,"data":{"content":"body"}}]}),
        &structured_options(10, 5, Some("response-v1:stale".into())),
    );
    assert_eq!(out["responsePagination"]["restart"], json!(true));
    assert_eq!(
        out["responseWindow"], "",
        "restart pages carry no envelope bytes"
    );
}

/// Whole-response paging delivers every byte or row exactly once at every
/// page size, including sizes smaller than one code point or one row.
/// `structured` windows concatenate to the exact source; `rows` pages are
/// complete JSON envelopes that together carry each match once. A rows
/// page stays inside its length unless it is one indivisible fragment.
#[test]
fn every_scope_delivers_each_byte_and_row_exactly_once_at_small_sizes() {
    let envelope = json!({"results":[
        {"index":0,"data":{"content":"alpha 😀 beta\ngamma 研究 delta"}},
        {"index":1,"data":{"content":"zeta é eta"}}
    ]});
    let serialized = envelope.to_string();
    for length in (1..=40).chain([64, 100, 257]) {
        let pages = walk_pages(&envelope, PageScope::Structured, length);
        let joined: String = pages
            .iter()
            .map(|page| page["responseWindow"].as_str().expect("window"))
            .collect();
        assert_eq!(joined, serialized, "structured length {length}");
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
        let pages = walk_pages(&rows, PageScope::Rows, length);
        let mut seen = Vec::new();
        let mut small = 0;
        for structured in &pages {
            let pagination = &structured["responsePagination"];
            // The whole page, pagination block included, fits.
            let rendered = rendered_units(structured);
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
            let pages = walk_pages(response, PageScope::Rows, length);
            assert_eq!(
                pages.len() > 1,
                rendered > length,
                "shape {shape} length {length}"
            );
            let mut seen = 0;
            for structured in &pages {
                let page = rendered_units(structured);
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
    let envelope = json!({"results":[{"index":0,"data":{"content":"body"}}]});
    let prepared = page_of("localFetch", &envelope, &ResponsePageOptions::default());
    assert_eq!(prepared, envelope);
    assert!(prepared.get("responseWindow").is_none());
}
