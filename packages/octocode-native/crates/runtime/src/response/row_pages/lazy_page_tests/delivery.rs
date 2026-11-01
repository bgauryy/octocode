use super::*;

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
                pagination.returned_chars <= length || pagination.oversized == Some(true),
                "length {length}: {} chars unflagged",
                pagination.returned_chars
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

/// A page whose batch has rows on later pages leads with the warning that
/// names the queries those pages serve or continue.
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
    let rows = first["results"].as_array().expect("rows");
    let index = |row: &Value| row["index"].as_u64().expect("index");
    let served = rows.iter().map(index).collect::<Vec<_>>();
    let continued = rows
        .iter()
        .filter(|row| row["rowPart"]["part"] != row["rowPart"]["of"])
        .map(index)
        .collect::<Vec<_>>();
    let unserved = (0..3)
        .filter(|index| !served.contains(index))
        .collect::<Vec<_>>();
    let list = |indexes: &[u64]| {
        let noun = if indexes.len() == 1 {
            "query"
        } else {
            "queries"
        };
        let indexes = indexes.iter().map(u64::to_string).collect::<Vec<_>>();
        format!("{noun} {}", indexes.join(", "))
    };
    assert!(!unserved.is_empty(), "{first:?}");
    let mut expected = format!("incomplete: later response pages serve {}", list(&unserved));
    if !continued.is_empty() {
        expected.push_str(&format!(" and continue {}", list(&continued)));
    }
    assert_eq!(banner, expected);
    // How to follow `next` lives once in the instructions, not per page.
    assert!(!banner.contains("responsePagination.next"), "{banner}");
    let (last, pagination) = pages.last().expect("last");
    assert!(!pagination.has_more);
    assert!(last.get("warnings").is_none(), "{last:?}");
    for (_, pagination) in &pages {
        assert!(pagination.oversized.is_none());
    }
}

/// Rows larger than a page are split, in row order: a page holds the
/// rest of one row before the next row starts, every page stays within the
/// budget, and each split row's page continuation rides its last part.
#[test]
fn split_rows_fill_pages_in_row_order() {
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
    let order = pages
        .iter()
        .flat_map(|(page, _)| {
            page["results"]
                .as_array()
                .expect("rows")
                .iter()
                .map(|row| (row["index"].as_u64(), row["rowPart"]["part"].as_u64()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(order, sorted, "rows and their parts in order");
    let mut carriers = std::collections::BTreeMap::new();
    for (page, pagination) in &pages {
        assert!(
            pagination.returned_chars <= 20_000,
            "{}",
            pagination.returned_chars
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

/// Rows that fit a page are never split: pages cut at row boundaries, so
/// twelve rows of 2,000 chars at 3,000 chars a page are twelve whole-row
/// pages, and `totalPages` is the twelve calls a reader makes.
#[test]
fn rows_that_fit_a_page_are_never_split() {
    let rows = (0..12)
        .map(|index| json!({"index": index, "data": {"content": "x".repeat(2_000)}}))
        .collect::<Vec<_>>();
    let structured = json!({"results": rows})
        .as_object()
        .cloned()
        .expect("object");
    let pages = walk(&structured, 3_000);
    assert_eq!(pages.len(), 12);
    for (index, (page, pagination)) in pages.iter().enumerate() {
        assert_eq!(pagination.total_pages, 12);
        let rows = page["results"].as_array().expect("rows");
        assert_eq!(rows.len(), 1, "{page:?}");
        assert!(rows[0].get("rowPart").is_none(), "{page:?}");
        assert_eq!(rows[0]["index"], index);
        assert!(pagination.oversized.is_none());
    }
}
