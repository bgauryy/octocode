use super::*;

#[test]
fn rows_scope_splits_nested_arrays_into_complete_json_pages() {
    let matches = (0..200)
        .map(|line| json!({"line":line,"value":"x".repeat(40)}))
        .collect::<Vec<_>>();
    let structured = json!({"results":[{"index":0,"data":{
        "stats":{"total":400},
        "files":[{"path":"a","matches":matches.clone()},{"path":"b","matches":matches}]
    }}]});
    let options = ResponsePageOptions {
        response_length: Some(2000),
        response_scope: Some(crate::response::cursor::PageScope::Rows),
        ..Default::default()
    };
    let (first, pagination) = paginate_rows(
        structured.as_object().unwrap().clone(),
        &options,
        Vec::new(),
        &[],
        None,
    );
    assert!(pagination.total_pages > 2 && pagination.has_more);
    let row = &first["results"][0];
    assert_eq!(
        row["data"]["stats"]["total"], 400,
        "non-split fields are kept"
    );
    assert_eq!(row["data"]["files"].as_array().unwrap().len(), 1);
    assert!(
        octocode_engine::json_size::utf16_units(&Value::Object(first.clone())).unwrap_or(0)
            <= 2000 + 200
    );
    let mut seen = 0;
    for page in 0..pagination.total_pages {
        let options = ResponsePageOptions {
            response_offset: Some(page),
            response_snapshot: Some(pagination.snapshot.clone()),
            ..options.clone()
        };
        let (envelope, _) = paginate_rows(
            structured.as_object().unwrap().clone(),
            &options,
            Vec::new(),
            &[],
            None,
        );
        for row in envelope["results"].as_array().unwrap() {
            for file in row["data"]["files"].as_array().unwrap() {
                seen += file["matches"].as_array().unwrap().len();
            }
        }
    }
    assert_eq!(seen, 400, "every match appears exactly once");
    let stale = ResponsePageOptions {
        response_offset: Some(1),
        response_snapshot: Some("response-rows-v1:stale".into()),
        ..options
    };
    let (_, restart) = paginate_rows(
        structured.as_object().unwrap().clone(),
        &stale,
        Vec::new(),
        &[],
        None,
    );
    assert_eq!(restart.restart, Some(true));
    assert_eq!(restart.next_char_offset, Some(0));
}

/// A rows-scope offset past the last page is a restart that names the
/// page asked for ("page 51 of N"), not page 1, and says so in `warnings`.
#[test]
fn rows_offset_past_the_end_restarts_naming_the_requested_page() {
    let matches = (1..=200)
        .map(|line| json!({"line":line,"value":"x".repeat(40)}))
        .collect::<Vec<_>>();
    let structured = json!({"results":[{"index":0,"data":{
        "files":[{"path":"a","matches":matches}]
    }}]});
    let options = ResponsePageOptions {
        response_length: Some(2000),
        response_scope: Some(crate::response::cursor::PageScope::Rows),
        ..Default::default()
    };
    let (_, first) = paginate_rows(
        structured.as_object().unwrap().clone(),
        &options,
        Vec::new(),
        &[],
        None,
    );
    let past = ResponsePageOptions {
        response_offset: Some(50),
        response_snapshot: Some(first.snapshot.clone()),
        ..options
    };
    let (envelope, restart) = paginate_rows(
        structured.as_object().unwrap().clone(),
        &past,
        Vec::new(),
        &[],
        None,
    );
    assert_eq!(restart.restart, Some(true));
    assert_eq!(restart.current_page, 51, "the page asked for");
    assert_eq!(restart.total_pages, first.total_pages);
    let reason = format!("page 51 of {}", first.total_pages);
    assert!(
        envelope["warnings"].as_array().is_some_and(|w| w
            .iter()
            .any(|w| w.as_str().is_some_and(|w| w.contains(&reason)))),
        "{envelope:?}"
    );
}

/// A later slice of a split pull request names it (its required
/// fields) and continues its comments; the long body rides part 1 once.
#[test]
fn later_slices_of_an_element_do_not_repeat_its_description() {
    let comments = (0..80)
        .map(|n| json!({"author":"a","body":format!("comment {n} {}", "x".repeat(60))}))
        .collect::<Vec<_>>();
    let body = "B".repeat(600);
    let structured = json!({"results":[{"index":0,"data":{
        "pullRequests":[{"number":7,"title":"t","state":"open","body":body,
            "headSha":"h","comments":comments,"reviews":[{"author":"r","state":"approved"}]}]
    }}]});
    let options = ResponsePageOptions {
        response_length: Some(3_000),
        response_scope: Some(crate::response::cursor::PageScope::Rows),
        ..Default::default()
    };
    let mut offset = 0;
    let mut bodies = 0;
    let mut seen = 0;
    loop {
        let page = ResponsePageOptions {
            response_offset: Some(offset),
            response_snapshot: (offset > 0).then(|| {
                paginate_rows(
                    structured.as_object().unwrap().clone(),
                    &options,
                    Vec::new(),
                    &[],
                    Some(crate::id::ToolId::GhGetHistoryItem),
                )
                .1
                .snapshot
            }),
            ..options.clone()
        };
        let (envelope, pagination) = paginate_rows(
            structured.as_object().unwrap().clone(),
            &page,
            Vec::new(),
            &[],
            Some(crate::id::ToolId::GhGetHistoryItem),
        );
        let pr = &envelope["results"][0]["data"]["pullRequests"][0];
        assert_eq!(pr["number"], 7, "{envelope:?}");
        bodies += usize::from(pr.get("body").is_some());
        seen += pr["comments"].as_array().map_or(0, Vec::len);
        assert_ne!(pr.get("reviews"), Some(&json!([])), "no emptied list: {pr}");
        if !pagination.has_more {
            break;
        }
        offset += 1;
    }
    assert_eq!(seen, 80);
    assert_eq!(bodies, 1, "the body rides one part");
}

/// A row with two large arrays (a comparison's commits and files)
/// delivers each array once; slices of one never repeat the other.
#[test]
fn rows_scope_never_repeats_a_second_large_array_per_fragment() {
    let commits = (0..100)
        .map(|n| json!({"sha":format!("{n:040}"),"messageHeadline":"x".repeat(40)}))
        .collect::<Vec<_>>();
    let files = (0..100)
        .map(|n| format!("M +1 -1 src/file_{n:03}.rs"))
        .collect::<Vec<_>>();
    let structured = json!({"results":[{"index":0,"data":{
        "type":"compare","aheadBy":100,"commits":commits,"files":files
    }}]});
    let full = structured.to_string();
    let options = ResponsePageOptions {
        response_length: Some(6_000),
        response_scope: Some(crate::response::cursor::PageScope::Rows),
        ..Default::default()
    };
    let (_, first) = paginate_rows(
        structured.as_object().unwrap().clone(),
        &options,
        Vec::new(),
        &[],
        None,
    );
    let (mut commits_seen, mut files_seen, mut chars) = (0, 0, 0);
    for page in 0..first.total_pages {
        let options = ResponsePageOptions {
            response_offset: Some(page),
            response_snapshot: Some(first.snapshot.clone()),
            ..options.clone()
        };
        let (envelope, _) = paginate_rows(
            structured.as_object().unwrap().clone(),
            &options,
            Vec::new(),
            &[],
            None,
        );
        chars +=
            octocode_engine::json_size::utf16_units(&Value::Object(envelope.clone())).unwrap_or(0);
        for row in envelope["results"].as_array().unwrap() {
            commits_seen += row["data"]["commits"].as_array().map_or(0, Vec::len);
            files_seen += row["data"]["files"].as_array().map_or(0, Vec::len);
        }
    }
    assert_eq!((commits_seen, files_seen), (100, 100));
    assert!(
        chars < full.len() + full.len() / 4,
        "{chars} vs {}",
        full.len()
    );
}

/// A skeleton (stats, pagination) that nearly fills the budget must not
/// explode one row into one fragment per match, each repeating it.
#[test]
fn rows_scope_does_not_repeat_a_dominant_skeleton_per_item() {
    let matches = (0..30)
        .map(|line| json!({"line":line,"value":"x".repeat(20)}))
        .collect::<Vec<_>>();
    let structured = json!({"results":[{"index":0,"data":{
        "stats":{"note":"s".repeat(300)},
        "files":[{"path":"a","matches":matches}]
    }}]});
    let options = ResponsePageOptions {
        response_length: Some(600),
        response_scope: Some(crate::response::cursor::PageScope::Rows),
        ..Default::default()
    };
    let (first, pagination) = paginate_rows(
        structured.as_object().unwrap().clone(),
        &options,
        Vec::new(),
        &[],
        None,
    );
    let items = first["results"][0]["data"]["files"][0]["matches"]
        .as_array()
        .map_or(0, Vec::len);
    assert!(items > 1, "each fragment carries several items: {first:?}");
    assert!(pagination.total_pages < 15, "{}", pagination.total_pages);
}
