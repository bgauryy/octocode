use super::{
    FetchCall, LineRange, LocalFetchQuery, NextCalls, ReadSource, line_read, local_fetch_query,
    test_local_fetch_query,
};
use crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;

#[test]
fn match_selection_preserves_empty_context_and_missed_matches() {
    use super::{LocalFetchResult, ReadStatus};

    for (content, count, hits) in [
        ("same\n", 1, vec![2]),
        ("same\n", 1, vec![]),
        ("", 0, vec![]),
    ] {
        let mut result = LocalFetchResult::blank(ReadStatus::Success);
        result.content = Some(content.into());
        result.selected_match_count = Some(count);
        result.matched_lines = hits.clone();
        let wire = serde_json::to_value(result).expect("read result");
        if content.is_empty() {
            // C3eK A-R9: a page with no text states no zero hit list or count.
            assert!(wire.get("matchedLines").is_none(), "{wire}");
            assert!(wire.get("selectedMatchCount").is_none(), "{wire}");
        } else {
            assert_eq!(wire["matchedLines"], serde_json::json!(hits), "{wire}");
        }
    }
    let plain = serde_json::to_value(LocalFetchResult::blank(ReadStatus::Success))
        .expect("unselected read");
    assert!(plain.get("matchedLines").is_none(), "{plain}");
}

/// A1-13: `matchedLines` restates the `NN\t` prefixes when every line the page
/// shows is a hit; it stays when context hides which lines hit.
#[test]
fn matched_lines_are_stated_only_when_context_hides_the_hits() {
    use super::{LocalFetchResult, ReadStatus};

    let wire = |ranges: &[(usize, usize)], hits: &[usize]| {
        let mut result = LocalFetchResult::blank(ReadStatus::Success);
        result.content = Some("text\n".into());
        result.selected_match_count = Some(hits.len());
        result.matched_lines = hits.to_vec();
        result.source_line_ranges = ranges
            .iter()
            .map(|&(start, end)| LineRange { start, end })
            .collect();
        serde_json::to_value(result).expect("read result")
    };
    // Every shown line is a hit: the numbered lines already say which.
    let all_hits = wire(&[(2, 2), (5, 6)], &[2, 5, 6]);
    assert!(all_hits.get("matchedLines").is_none(), "{all_hits}");
    assert_eq!(all_hits["selectedMatchCount"], 3, "{all_hits}");
    // Context lines are shown: the hits are not derivable.
    let with_context = wire(&[(1, 3)], &[2]);
    assert_eq!(
        with_context["matchedLines"],
        serde_json::json!([2]),
        "{with_context}"
    );
    // Nothing states which lines the text holds: keep the hits.
    let unlocated = wire(&[], &[2]);
    assert_eq!(
        unlocated["matchedLines"],
        serde_json::json!([2]),
        "{unlocated}"
    );
}

/// Every lead `line_read` builds (continueBlock,
/// readBoundedLines) spells lines with the published `ranges`, never
/// startLine/endLine, and clears a span the caller sent.
#[test]
fn line_reads_spell_published_ranges() {
    let mut q = test_local_fetch_query();
    local_fetch_query::set_line_span(&mut q, 1, 9).expect("range");
    let wire = |query: LocalFetchQuery| serde_json::to_value(query).expect("query");
    let one = wire(line_read(&q, &[LineRange { start: 57, end: 69 }]).expect("read"));
    assert_eq!(
        one["ranges"],
        serde_json::json!([{"startLine":57,"endLine":69}]),
        "{one}"
    );
    assert!(
        one.get("startLine").is_none() && one.get("endLine").is_none(),
        "{one}"
    );
    let many = (0..=RANGES_MAX_ITEMS)
        .map(|i| LineRange {
            start: 10 * i + 1,
            end: 10 * i + 2,
        })
        .collect::<Vec<_>>();
    // One range past the cap: the read spans the first start to the last end.
    let span = wire(line_read(&q, &many).expect("read"));
    assert_eq!(
        span["ranges"],
        serde_json::json!([{"startLine":1,"endLine":10 * RANGES_MAX_ITEMS + 2}]),
        "{span}"
    );
    assert!(span.get("startLine").is_none(), "{span}");
}

#[test]
fn read_range_cap_comes_from_the_contract() {
    let declared = crate::contracts::query_schema_number(
        crate::id::ToolId::LocalFetch,
        None,
        "ranges",
        "maxItems",
    )
    .expect("localFetch ranges declares maxItems");
    assert_eq!(RANGES_MAX_ITEMS as u64, declared);
}

/// The read list names each read as it serializes, covers every read
/// field, and decides `leaves_unread` and the source binding alone.
#[test]
fn the_read_list_is_every_read_by_its_wire_name() {
    let query = serde_json::from_value(serde_json::json!({"path": "a.txt"})).expect("query");
    let call = FetchCall::continued(query);
    let mut next = NextCalls::default();
    for (_, _, read) in next.reads_slots() {
        *read = Some(call.clone());
    }
    let wire = serde_json::to_value(&next).expect("next");
    let names = next.reads().map(|(name, _, _)| name);
    assert_eq!(
        wire.as_object()
            .expect("object")
            .keys()
            .collect::<Vec<_>>()
            .len(),
        names.len(),
        "{wire}"
    );
    for name in names {
        assert!(
            wire.get(name).is_some(),
            "{name} serializes as listed: {wire}"
        );
    }
    assert!(next.leaves_unread());
    let lead = NextCalls {
        read: Some(FetchCall::related(call.query.clone())),
        restart: Some(FetchCall::related(call.query.clone())),
        ..NextCalls::default()
    };
    assert!(
        !lead.leaves_unread(),
        "a lead or a restart reads no more of the result"
    );
    let mut page = NextCalls {
        continue_window: Some(call.clone()),
        restart: Some(FetchCall::related(call.query.clone())),
        ..NextCalls::default()
    };
    assert_eq!(page.reads_of_mut(ReadSource::Page).count(), 1);
    assert!(
        !page.leaves_unread(),
        "the next window is pagination, not unread evidence"
    );
}
