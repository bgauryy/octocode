//! The localFetch wire form: derivable metadata omitted, invalid input
//! refused.

use super::*;

#[test]
fn wire_form_omits_metadata_derivable_from_emitted_fields() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "zero\none\nneedle\nthree\nfour\n")
        .expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some(MatchString::String("needle".parse().expect("match string")));
    req.context_lines = Some(1);
    let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
    assert_eq!(wire["content"], "one\nneedle\nthree\n");
    assert_eq!(
        wire["sourceLineRanges"],
        serde_json::json!([{"line": 2, "endLine": 4}])
    );
    assert_eq!(wire["matchedLines"], serde_json::json!([3]));
    // A stats field: extras project `matchedLines` (details) separately.
    assert_eq!(wire["selectedMatchCount"], 1);
    assert_eq!(wire["totalLines"], 5);
    assert_eq!(wire["sourceBytes"], 27);
    assert_eq!(wire["returnedBytes"], 17);
    for redundant in [
        "contentView",
        "startLine",
        "endLine",
        "matchRanges",
        "resolvedPath",
    ] {
        assert!(wire.get(redundant).is_none(), "{redundant}: {wire}");
    }
    // The read always states its window; the response stage keeps the block
    // only where it routes (a window with more, or past the first), so a
    // single complete window is absent from the published row.
    assert_eq!(wire["pagination"]["hasMore"], false, "{wire}");
    let published = crate::response::rows::finish_row(
        crate::id::ToolId::LocalFetch,
        0,
        &crate::response::result::query_value(&req),
        wire.clone(),
        None,
        crate::response::rows::RowParts::default(),
    );
    assert!(published["data"].get("pagination").is_none(), "{published}");
    // Explanation counters are built for every read; the contract classes
    // them `stats` extras, so a response shows them only under `extras:["stats"]`.
    let verbose = crate::response::extras::all_paths(crate::id::ToolId::LocalFetch);
    for counter in [
        "modified",
        "sourceChars",
        "sourceBytes",
        "returnedChars",
        "returnedBytes",
        "returnedLines",
    ] {
        let path = format!("results[].data.{counter}");
        assert!(verbose.contains(&path.as_str()), "{counter} is not verbose");
    }

    let mut paged = q(&p);
    paged.length = wire_positive(2);
    let wire = serde_json::to_value(fetch(&paged, &paths)).expect("serializable");
    assert_eq!(
        wire["pagination"],
        serde_json::json!({"unit":"lines","offset":0,"length":2,"hasMore":true,"nextOffset":2})
    );
    assert!(wire["next"]["continueWindow"].is_object());
    paged.offset = Some(4);
    let last = serde_json::to_value(fetch(&paged, &paths)).expect("serializable");
    assert_eq!(
        last["pagination"],
        serde_json::json!({"unit":"lines","offset":4,"length":1,"hasMore":false})
    );

    let wide = t.0.join("wide.txt");
    fs::write(&wide, "a😀\n").expect("test fixture operation should succeed");
    let wire = serde_json::to_value(fetch(&q(&wide), &paths)).expect("serializable");
    assert_eq!(wire["sourceChars"], 4);
    assert_eq!(wire["sourceBytes"], 6);
    assert_eq!(wire["returnedChars"], 4);
}

#[test]
fn schema_rejects_unknown_fields_and_invalid_combinations() {
    assert!(
        serde_json::from_value::<LocalFetchQuery>(serde_json::json!({"path":"x","madeUp":true}))
            .is_err()
    );
    // The contract is the one validator of selector combinations.
    let combined = serde_json::json!({"path":"x","fullContent":true,"matchString":"x"});
    assert!(crate::contracts::validate_query("localFetch", combined).is_err());
}
