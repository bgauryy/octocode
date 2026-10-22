//! Row-page planning and paging tests.

use super::*;

#[test]
fn declared_nested_collections_preserve_atomic_answers_and_shared_definitions() {
    let mut resources = Map::new();
    let mut expected = std::collections::BTreeMap::new();
    for resource in ["0", "r/~other"] {
        let sections = (0..3).map(|section| {
                let answers = (0..7).map(|question| {
                    let key = format!("q/{question}");
                    let probabilities = (0..13).map(|option| (format!("option-{option}"), json!(1.0 / 13.0))).collect::<Map<_, _>>();
                    let answer = json!({"status":"answered","answer":{
                        "type":"choice", "choice":"option-0", "confidence":0.0,
                        "probabilities":probabilities
                    }});
                    expected.insert((resource.to_owned(), format!("s{section}"), key.clone()), answer.clone());
                    (key, answer)
                }).collect::<Map<_, _>>();
                json!({"id":format!("s{section}"),"target":{"bytes":[section*100,(section+1)*100],"lines":[section*5+1,(section+1)*5]},
                    "context":[{"bytes":[0,5],"lines":[1,1]}],"answers":answers})
            }).collect::<Vec<_>>();
        resources.insert(
            resource.into(),
            json!({"status":"ready","snapshot":"fixed","sections":sections}),
        );
    }
    let row = json!({"id":"research","operation":"sections","resources":resources,
            "questions":(0..7).map(|question| (format!("q/{question}"), json!({"type":"choice","criteria":(0..13).map(|option| (format!("option-{option}"), Value::Null)).collect::<Map<_,_>>()}))).collect::<Map<_,_>>(),
            "next":{"nextSectionPage":{"tool":"clasify","query":{"queries":[{"operation":"sections","assessment":"next"}]}}}});
    let plan = RowPlan::new(vec![row], &[None], 1_800, Some(crate::id::ToolId::Clasify));
    assert!(plan.total_pages > 1);
    let mut seen = std::collections::BTreeMap::new();
    let mut definitions = 0;
    let mut next_calls = 0;
    for page in 0..plan.total_pages {
        for row in plan.build_page(page, &[None], &[]) {
            definitions += usize::from(row.get("questions").is_some());
            next_calls += usize::from(row["next"].get("nextSectionPage").is_some());
            if row["next"].get("nextSectionPage").is_some() {
                assert_eq!(row["rowPart"]["part"], row["rowPart"]["of"]);
            }
            for (resource, value) in row["resources"].as_object().expect("resources") {
                for section in value["sections"].as_array().expect("sections") {
                    assert_eq!(
                        section["target"]["bytes"].as_array().expect("bytes").len(),
                        2
                    );
                    assert_eq!(
                        section["target"]["lines"].as_array().expect("lines").len(),
                        2
                    );
                    assert_eq!(section["context"][0]["bytes"], json!([0, 5]));
                    for (question, answer) in section["answers"].as_object().expect("answers") {
                        let key = (
                            resource.clone(),
                            section["id"].as_str().expect("id").to_owned(),
                            question.clone(),
                        );
                        assert!(
                            seen.insert(key, answer.clone()).is_none(),
                            "duplicate answer"
                        );
                    }
                }
            }
        }
    }
    assert_eq!(seen, expected);
    assert_eq!(definitions, 1);
    assert_eq!(next_calls, 1);
}

#[test]
fn declared_collections_leave_a_single_oversized_answer_whole() {
    let probabilities = (0..100)
        .map(|index| (format!("option-{index}-{}", "x".repeat(48)), json!(0.01)))
        .collect::<Map<_, _>>();
    let answer = json!({"status":"answered","answer":{"type":"choice","choice":probabilities.keys().next().expect("option"),"confidence":0.0,"probabilities":probabilities}});
    let row = json!({"id":"research","operation":"sections","questions":{"q":{"type":"choice"}},
            "resources":{"r":{"sections":[{"id":"s","target":{"bytes":[0,1],"lines":[1,1]},"answers":{"q":answer}}]}}});
    let plan = RowPlan::new(vec![row], &[None], 500, Some(crate::id::ToolId::Clasify));
    assert_eq!(plan.total_pages, 1);
    let rows = plan.build_page(0, &[None], &[]);
    assert_eq!(
        rows[0]["resources"]["r"]["sections"][0]["answers"]["q"],
        answer
    );
}

#[test]
fn large_resource_identity_does_not_force_one_page_per_small_answer() {
    let answers = (0..25)
        .map(|index| {
            (
                format!("q{index}"),
                json!({
                    "status":"answered","answer":{"type":"noul","probability":0.5}
                }),
            )
        })
        .collect::<Map<_, _>>();
    let row = json!({"id":"research","operation":"sections",
            "resources":{"r":{"source":{"url":format!("https://example.test/{}","x".repeat(4000))},
            "sections":[{"id":"s","target":{"bytes":[0,1],"lines":[1,1]},"answers":answers}]}}});
    let plan = RowPlan::new(vec![row], &[None], 500, Some(crate::id::ToolId::Clasify));
    assert_eq!(plan.total_pages, 1);
    let rows = plan.build_page(0, &[None], &[]);
    assert_eq!(
        rows[0]["resources"]["r"]["sections"][0]["answers"],
        json!(answers)
    );
}

#[test]
fn answer_fragments_continue_their_only_section() {
    let answers = (0..30)
        .map(|index| {
            (
                format!("q{index}"),
                json!({
                    "status":"answered","answer":{"type":"noul","probability":0.5}
                }),
            )
        })
        .collect::<Map<_, _>>();
    let row = json!({"id":"research","operation":"sections",
            "resources":{"r/~0":{"sections":[{"id":"s","target":{"bytes":[0,1],"lines":[1,1]},"answers":answers}]}}});
    let plan = RowPlan::new(vec![row], &[None], 600, Some(crate::id::ToolId::Clasify));
    assert!(plan.total_pages > 1);
    let mut seen = Map::new();
    let mut fragments = 0;
    for page in 0..plan.total_pages {
        for row in plan.build_page(page, &[None], &[]) {
            if fragments > 0 {
                assert_eq!(
                    row["rowPart"]["continues"],
                    json!({"array":"/resources/r~1~00/sections","index":0})
                );
            }
            for (key, value) in row["resources"]["r/~0"]["sections"][0]["answers"]
                .as_object()
                .expect("answers")
            {
                assert!(seen.insert(key.clone(), value.clone()).is_none());
            }
            fragments += 1;
        }
    }
    assert_eq!(seen, answers);
}

#[test]
fn numeric_resource_keys_are_not_array_indexes() {
    let row = json!({"resources":{"0":{"sections":[{"answers":{}}]}}});
    assert_eq!(
        enclosing_element(&row, "/resources/0/sections/0/answers"),
        Some(("/resources/0/sections".into(), 0))
    );
}

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
        response_scope: Some("rows".into()),
        ..Default::default()
    };
    let full = structured.to_string();
    let (first, pagination) = paginate_rows(
        structured.as_object().unwrap().clone(),
        &full,
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
            &full,
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
        &full,
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
        response_scope: Some("rows".into()),
        ..Default::default()
    };
    let full = structured.to_string();
    let (_, first) = paginate_rows(
        structured.as_object().unwrap().clone(),
        &full,
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
        &full,
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
        response_scope: Some("rows".into()),
        ..Default::default()
    };
    let full = structured.to_string();
    let mut offset = 0;
    let mut bodies = 0;
    let mut seen = 0;
    loop {
        let page = ResponsePageOptions {
            response_offset: Some(offset),
            response_snapshot: (offset > 0).then(|| {
                paginate_rows(
                    structured.as_object().unwrap().clone(),
                    &full,
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
            &full,
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
    let options = ResponsePageOptions {
        response_length: Some(6_000),
        response_scope: Some("rows".into()),
        ..Default::default()
    };
    let full = structured.to_string();
    let (_, first) = paginate_rows(
        structured.as_object().unwrap().clone(),
        &full,
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
            &full,
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
        response_scope: Some("rows".into()),
        ..Default::default()
    };
    let full = structured.to_string();
    let (first, pagination) = paginate_rows(
        structured.as_object().unwrap().clone(),
        &full,
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
