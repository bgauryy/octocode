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
        assert_eq!(wire["matchedLines"], serde_json::json!(hits), "{wire}");
    }
    let plain = serde_json::to_value(LocalFetchResult::blank(ReadStatus::Success))
        .expect("unselected read");
    assert!(plain.get("matchedLines").is_none(), "{plain}");
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
    let span = wire(line_read(&q, &many).expect("read"));
    assert_eq!(
        span["ranges"],
        serde_json::json!([{"startLine":1,"endLine":102}]),
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
/// field, and decides `leaves_more` and the source binding alone.
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
    assert!(next.leaves_more());
    let lead = NextCalls {
        read: Some(FetchCall::related(call.query.clone())),
        restart: Some(FetchCall::related(call.query.clone())),
        ..NextCalls::default()
    };
    assert!(
        !lead.leaves_more(),
        "a lead or a restart reads no more of the result"
    );
    let mut page = NextCalls {
        continue_window: Some(call.clone()),
        restart: Some(call),
        ..NextCalls::default()
    };
    assert_eq!(page.reads_of_mut(ReadSource::Page).count(), 1);
}
