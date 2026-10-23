use super::*;

/// A stale snapshot names every cause: the source or the query changed,
/// or the snapshot is missing or from another call.
#[test]
fn stale_snapshot_names_source_or_query() {
    for cause in ["source and query", "changed", "missing", "another call"] {
        assert!(
            crate::response::pages::STALE_SNAPSHOT_ERROR.contains(cause),
            "{cause}"
        );
    }
    assert!(!crate::response::pages::STALE_SNAPSHOT_ERROR.contains("next."));
    // The next call: the row's one lead, `restart`.
    assert!(crate::response::pages::STALE_SNAPSHOT_ERROR.contains("restart"));
}

/// A restart the tool did not build is the one restart owner's: page 1
/// of the same query without any cursor or saved selection, a replay.
#[test]
fn a_runtime_restart_drops_cursors_and_the_saved_selection() {
    let query = json!({"path":"a.rs","offset":90,"snapshot":"s","selection":"plan"});
    let mut data = json!({"errorCode":"staleSnapshot","isPartial":true});
    super::super::restart_stale(ToolId::LocalFetch, &query, &mut data);
    assert_eq!(
        data["next"]["restart"],
        crate::response::result::restart_query(ToolId::LocalFetch, &query),
        "{data}"
    );
    assert_eq!(
        data["next"]["restart"]["query"]["queries"][0],
        json!({"path":"a.rs"})
    );
    assert_eq!(data["next"]["restart"]["confidence"], "exact");
}

/// A content window states where it stands in its unit: the shown
/// span, the total when known, and the next offset while more is left.
#[test]
fn span_facts_state_one_window_shape() {
    assert_eq!(
        SpanFacts::window("chars", 0, 12_000, 14_366)
            .to_value()
            .to_string(),
        r#"{"unit":"chars","offset":0,"length":12000,"total":14366,"hasMore":true,"nextOffset":12000}"#
    );
    let last = SpanFacts::window("chars", 12_000, 2_366, 14_366);
    assert!(!last.has_more());
    assert_eq!(last.next_offset(), None);
    assert_eq!(
        last.to_value().to_string(),
        r#"{"unit":"chars","offset":12000,"length":2366,"total":14366,"hasMore":false}"#
    );
    // A window is clamped to its view: past the end it is empty at the end.
    assert_eq!(
        SpanFacts::window("bytes", 900, 50, 100).to_value(),
        json!({"unit":"bytes","offset":100,"length":0,"total":100,"hasMore":false})
    );
    assert_eq!(SpanFacts::window("bytes", 90, 50, 100).next_offset(), None);
    let block = SpanFacts::window("lines", 0, 1, 2)
        .out_of_range(true)
        .to_value();
    let keys: Vec<&str> = block
        .as_object()
        .expect("block")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, SpanFacts::KEYS, "every key, in block order");
    // A view whose total the row states leaves it off the window; a
    // window past the end is flagged.
    assert_eq!(
        SpanFacts::window("lines", 0, 5, 5)
            .without_total()
            .to_value(),
        json!({"unit":"lines","offset":0,"length":5,"hasMore":false})
    );
    assert_eq!(
        SpanFacts::window("lines", 9, 5, 5)
            .out_of_range(true)
            .to_value(),
        json!({"unit":"lines","offset":5,"length":0,"total":5,"hasMore":false,"outOfRange":true})
    );
}

#[test]
fn page_facts_count_only_the_paged_unit() {
    // One key order on every page; the total is the paged unit's.
    assert_eq!(
        PageFacts::counted(2, 10, 25).to_value().to_string(),
        r#"{"currentPage":2,"totalPages":3,"pageSize":10,"totalItems":25,"hasMore":true}"#
    );
    assert_eq!(
        PageFacts::counted(3, 10, 25).to_value()["hasMore"],
        json!(false)
    );
    // A page past the end is a page, not more.
    assert_eq!(
        PageFacts::counted(4, 10, 25).to_value()["hasMore"],
        json!(false)
    );
    // Unknown totals state only the cursor facts, in the same order.
    assert_eq!(
        PageFacts::open(1, Some(30), true).to_value().to_string(),
        r#"{"currentPage":1,"pageSize":30,"hasMore":true}"#
    );
    assert_eq!(
        PageFacts::open(1, Some(30), true).with_total(31).to_value(),
        json!({"currentPage":1,"totalPages":2,"pageSize":30,"totalItems":31,"hasMore":true})
    );
    // Size-cut pages have no page size.
    assert_eq!(
        PageFacts::sized(1, 4, 90, true).to_value().to_string(),
        r#"{"currentPage":1,"totalPages":4,"totalItems":90,"hasMore":true}"#
    );
    // A size-cut page keeps its byte-derived page count when its total is
    // restated; only a page without one derives it from the count.
    assert_eq!(
        PageFacts::sized(1, 4, 90, true)
            .with_page_size(10)
            .with_total(90)
            .to_value()["totalPages"],
        json!(4)
    );
}

/// Keys any branch of the shared output `pagination` schema declares,
/// for every tool, plus the cursor keys the stage moves onto `next`.
fn declared_pagination_keys() -> std::collections::BTreeMap<&'static str, Vec<String>> {
    ToolId::ALL
        .iter()
        .filter_map(|tool| {
            let contract = crate::contracts::tool_contract(*tool).expect("tool contract");
            let pagination = contract["outputSchema"]["$defs"].get("OutputPagination")?;
            let mut keys = pagination["anyOf"]
                .as_array()
                .map_or_else(|| vec![pagination], |branches| branches.iter().collect())
                .into_iter()
                .flat_map(|branch| branch["properties"].as_object().into_iter().flatten())
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            assert!(
                !keys.is_empty(),
                "{} declares output pagination",
                tool.as_str()
            );
            keys.extend(super::super::CURSOR_KEYS.map(str::to_owned));
            Some((tool.as_str(), keys))
        })
        .collect()
}

/// Every key native writes into a response `pagination` block is one the
/// contract declares for every tool (the block is one shared schema): a
/// `PageFacts` block with every fact set, and every literal write
/// `…["pagination"]["key"] =` or through a `get_mut("pagination")`
/// binding in the runtime's non-test sources.
#[test]
fn every_emitted_pagination_key_is_declared_by_the_contract() {
    let declared = declared_pagination_keys();
    assert!(
        declared.len() > 10,
        "tools with output pagination: {declared:?}"
    );
    let mut emitted = PageFacts::counted(2, 5, 40)
        .out_of_range(true)
        .to_value()
        .as_object()
        .expect("block")
        .keys()
        .map(|key| (key.clone(), "PageFacts".to_owned()))
        .collect::<Vec<_>>();
    emitted.extend(SpanFacts::KEYS.map(|key| (key.to_owned(), "SpanFacts".to_owned())));
    let direct = regex::Regex::new(r#"\["pagination"\]\["(\w+)"\]\s*=[^=]"#).expect("regex");
    let binding =
        regex::Regex::new(r#"let Some\((\w+)\) = [^;{]*get_mut\("pagination"\)"#).expect("regex");
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut dirs = vec![src];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).expect("src dir") {
            let path = entry.expect("entry").path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if path.is_dir() {
                if name != "tests" {
                    dirs.push(path);
                }
                continue;
            }
            if !name.ends_with(".rs") || name == "tests.rs" || name.ends_with("_tests.rs") {
                continue;
            }
            let text = crate::tools::test_support::production_source(&path);
            let at = path.display().to_string();
            emitted.extend(
                direct
                    .captures_iter(&text)
                    .map(|capture| (capture[1].to_owned(), at.clone())),
            );
            for bound in binding.captures_iter(&text) {
                let write = regex::Regex::new(&format!(r#"\b{}\["(\w+)"\]\s*=[^=]"#, &bound[1]))
                    .expect("regex");
                emitted.extend(
                    write
                        .captures_iter(&text)
                        .map(|capture| (capture[1].to_owned(), at.clone())),
                );
            }
        }
    }
    let undeclared = emitted
        .iter()
        .flat_map(|(key, at)| {
            declared
                .iter()
                .filter(move |(_, keys)| !keys.contains(key))
                .map(move |(tool, _)| format!("{at}: pagination.{key} ({tool})"))
        })
        .collect::<Vec<_>>();
    assert!(
        undeclared.is_empty(),
        "undeclared pagination keys: {undeclared:#?}"
    );
    assert!(emitted.len() > 5, "the scan finds the writes: {emitted:?}");
}
