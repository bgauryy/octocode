use super::*;

#[test]
fn changing_the_row_page_budget_requires_a_restart() {
    let structured = envelope();
    let (_, first) = paginate_rows(
        structured.clone(),
        &options(1_000, 0, None),
        Vec::new(),
        &[],
        None,
    );
    assert!(first.has_more);
    for changed_length in [700, 2_000] {
        let (page, pagination) = paginate_rows(
            structured.clone(),
            &options(changed_length, 1, Some(first.snapshot.clone())),
            Vec::new(),
            &[],
            None,
        );
        assert_eq!(pagination.restart, Some(true), "length {changed_length}");
        assert_eq!(pagination.changed, Some(true));
        assert_eq!(pagination.next_char_offset, Some(0));
        assert_eq!(page["results"], json!([]));
    }
}

#[test]
fn an_earlier_row_plan_version_requires_a_restart() {
    let structured = envelope();
    let (_, first) = paginate_rows(
        structured.clone(),
        &options(1_000, 0, None),
        Vec::new(),
        &[],
        None,
    );
    assert!(first.snapshot.starts_with("response-rows-v6:"));
    let obsolete = first
        .snapshot
        .replacen("response-rows-v6:", "response-rows-v5:", 1);
    let (page, pagination) = paginate_rows(
        structured,
        &options(1_000, 1, Some(obsolete)),
        Vec::new(),
        &[],
        None,
    );
    assert_eq!(pagination.restart, Some(true));
    assert_eq!(pagination.next_char_offset, Some(0));
    assert_eq!(page["results"], json!([]));
}

#[test]
fn every_row_is_delivered_exactly_once_within_the_page_bound() {
    // Every page re-plans every row, so the rows are only as broad as the
    // lengths need: a broad row still spans several pages at the largest
    // length below that splits it (9_000).
    const MATCHES: usize = 200;
    let structured = envelope_of(MATCHES);
    let broad = structured["results"][2].to_string().len();
    assert!(broad > 9_000, "a broad row ({broad} chars) splits at 9_000");
    let mut expected: Vec<(u64, String, u64)> = (0..MATCHES as u64)
        .flat_map(|line| [(0, "a.rs".to_owned(), line), (2, "c.rs".to_owned(), line)])
        .chain([(0, "b.rs".to_owned(), 9999)])
        .collect();
    expected.sort();
    for length in [300, 700, 2_000, 9_000, 200_000] {
        let pages = walk(&structured, length);
        assert_eq!(delivered(&pages), expected, "length {length}");
        let mut parts: std::collections::BTreeMap<u64, Vec<(u64, u64)>> = Default::default();
        let mut small = 0;
        for (page, pagination) in &pages {
            assert!(
                pagination.char_length <= length || pagination.oversized == Some(true),
                "length {length}: {} chars unflagged",
                pagination.char_length
            );
            for row in page["results"].as_array().expect("rows") {
                small += usize::from(row["data"]["content"] == "small");
                let index = row["index"].as_u64().expect("index");
                let part = (
                    row["rowPart"]["part"].as_u64().unwrap_or(1),
                    row["rowPart"]["of"].as_u64().unwrap_or(1),
                );
                parts.entry(index).or_default().push(part);
            }
        }
        assert_eq!(small, 1, "length {length}");
        for (index, seen) in parts {
            let of = seen[0].1;
            assert_eq!(
                seen,
                (1..=of).map(|part| (part, of)).collect::<Vec<_>>(),
                "length {length} row {index}"
            );
        }
    }
}

/// One broad row must not fill the first pages alone; every row of a
/// batch shows its head on page one, within the budget. Each split row's
/// page continuation waits for its last part.
#[test]
fn a_page_with_rows_left_over_leads_with_an_incomplete_warning() {
    let rows = (0..3)
        .map(|index| json!({"index": index, "data": {"content": "x".repeat(9_000)}}))
        .collect::<Vec<_>>();
    let structured = json!({"results": rows})
        .as_object()
        .cloned()
        .expect("object");
    let pages = walk(&structured, 12_000);
    assert!(pages.len() > 1);
    let (first, _) = &pages[0];
    assert_eq!(first.keys().next().map(String::as_str), Some("warnings"));
    let banner = first["warnings"][0].as_str().expect("banner");
    assert!(
        banner.starts_with("incomplete — 2 of 3 rows continue"),
        "{banner}"
    );
    // How to follow `next` lives once in the instructions, not per page.
    assert!(!banner.contains("responsePagination.next"), "{banner}");
    let (last, pagination) = pages.last().expect("last");
    assert!(!pagination.has_more);
    assert!(last.get("warnings").is_none(), "{last:?}");
    for (_, pagination) in &pages {
        assert!(pagination.oversized.is_none());
    }
}

#[test]
fn the_first_page_shows_every_rows_head() {
    let rows = (0..5)
            .map(|index| {
                let matches = (0..300)
                    .map(|line| json!({"line": line, "value": format!("row {index} {}", "x".repeat(60))}))
                    .collect::<Vec<_>>();
                json!({"index": index, "data": {"files": [{"path": format!("f{index}.rs"), "matches": matches}],
                    "next": {"nextPage": {"tool": "localSearch", "query": {"page": 2}}}}})
            })
            .collect::<Vec<_>>();
    let structured = json!({"results": rows})
        .as_object()
        .cloned()
        .expect("object");
    let pages = walk(&structured, 20_000);
    let (first, pagination) = &pages[0];
    assert!(
        pagination.char_length <= 20_000,
        "{}",
        pagination.char_length
    );
    let heads: Vec<_> = first["results"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| (row["index"].as_u64(), row["rowPart"]["part"].as_u64()))
        .collect();
    assert_eq!(
        heads,
        (0..5)
            .map(|index| (Some(index), Some(1)))
            .collect::<Vec<_>>()
    );
    let mut carriers = std::collections::BTreeMap::new();
    for (page, pagination) in &pages {
        assert!(
            pagination.char_length <= 20_000,
            "{}",
            pagination.char_length
        );
        assert!(pagination.oversized.is_none(), "{page:?}");
        for row in page["results"].as_array().expect("rows") {
            let part = row["rowPart"]["part"].as_u64().expect("split");
            let of = row["rowPart"]["of"].as_u64().expect("split");
            if row["data"].get("next").is_some() {
                assert_eq!(part, of, "{row}");
                *carriers.entry(row["index"].as_u64()).or_insert(0) += 1;
            }
        }
    }
    assert_eq!(carriers.len(), 5, "{carriers:?}");
    assert!(carriers.values().all(|count| *count == 1), "{carriers:?}");
    assert_eq!(delivered(&pages).len(), 1_500);
}
