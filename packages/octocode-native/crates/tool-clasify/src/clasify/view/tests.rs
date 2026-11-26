use super::*;
use octocode_test_support::json_fixture;

/// Every leaf a summary states is a leaf of the details row: a summary
/// only removes, or unwraps an answered cell to its answer.
fn within(summary: &Value, details: &Value) -> bool {
    match (summary, details) {
        (Value::Object(summary), Value::Object(details)) => {
            if details.get("status") == Some(&json!("answered")) && !summary.contains_key("status")
            {
                return within(&Value::Object(summary.clone()), &details["answer"]);
            }
            summary
                .iter()
                .all(|(key, value)| details.get(key).is_some_and(|other| within(value, other)))
        }
        (Value::Array(summary), Value::Array(details)) => summary
            .iter()
            .all(|value| details.iter().any(|other| within(value, other))),
        _ => summary == details,
    }
}

fn summary_of(details: &Value) -> Value {
    let mut summary = details.clone();
    summarize(&mut summary, false, false);
    // The one thing a summary adds is the warning that names what it
    // left out; everything else it states is a leaf of the details.
    let mut stated = summary.clone();
    if let Some(fields) = stated.as_object_mut() {
        fields.shift_remove("warnings");
    }
    assert!(within(&stated, details), "{summary}\n{details}");
    summary
}

#[test]
fn a_predicate_summary_preserves_every_assessed_score_through_paging() {
    use octocode_io::cancellation::Cancellation;
    use octocode_runtime_base::response::{
        page_model::ResponsePageOptions,
        pager::{ResponseInput, SavedPages, prepare_saved},
    };
    use std::collections::BTreeMap;

    let hits: Map<String, Value> = (1..=64)
        .map(|line| {
            (
                line.to_string(),
                json!({"p1":{"noul":f64::from(line) / 256.0}}),
            )
        })
        .collect();
    let units: Map<String, Value> = (1..=64)
        .map(|id| {
            (
                format!("repo-{id}"),
                json!({"p1":{"noul":f64::from(id) / 512.0}}),
            )
        })
        .collect();
    let quiet = hits.len() + units.len();
    let usage = json!({"calls":65,"inputTokens":65,"outputTokens":65});
    let mut row = json!({
        "files":{"src/fixture.ts":hits}, "units":units,
        "coverage":{"totalUnits":quiet,"answered":quiet,"quiet":quiet}, "usage":usage,
    });
    summarize(&mut row, true, false);
    assert_eq!(row["coverage"]["quiet"], quiet);
    assert_eq!(row["usage"], usage);
    assert!(row.get("warnings").is_none());

    let envelope = json!({"results":[{"index":0,"data":row}]});
    let budget = 800;
    let mut input = ResponseInput {
        tool: ToolId::Clasify,
        structured: Value::Null,
        options: ResponsePageOptions {
            response_chars: Some(budget),
            paging: true,
            ..Default::default()
        },
    };
    let pages = SavedPages::default();
    let cancelled = Cancellation::new();
    let mut found = BTreeMap::new();
    let mut previous = None;
    loop {
        let page = prepare_saved(&input, &envelope, &pages, &cancelled).expect("saved page");
        let data = &page.value["results"][0]["data"];
        for (origin, items) in [
            ("file", &data["files"]["src/fixture.ts"]),
            ("unit", &data["units"]),
        ] {
            for (key, score) in items.as_object().into_iter().flatten() {
                assert!(
                    found.insert((origin, key.clone()), score.clone()).is_none(),
                    "duplicate {origin}/{key}"
                );
            }
        }
        let size = page.value.to_string().encode_utf16().count();
        assert!(
            size <= budget || page.value["oversized"] == true,
            "undisclosed oversized page ({size})"
        );
        let Some(next) = page.next else {
            break;
        };
        assert!(previous.is_none_or(|offset| next.offset > offset));
        previous = Some(next.offset);
        input.options.open(&next);
    }
    assert!(previous.is_some(), "fixture must exercise pagination");
    let expected: BTreeMap<_, _> = hits
        .into_iter()
        .map(|(key, score)| (("file", key), score))
        .chain(units.into_iter().map(|(key, score)| (("unit", key), score)))
        .collect();
    assert_eq!(found, expected);
}

/// What a summary states of a row's `usage`: the requests it billed, the
/// tokens they cost and its ceiling, as the details row states them.
fn billed(usage: &Value) -> Value {
    let mut billed = usage.clone();
    if let Some(fields) = billed.as_object_mut() {
        fields.retain(|field, _| {
            matches!(
                field.as_str(),
                "calls" | "inputTokens" | "outputTokens" | "requestsCeiling"
            )
        });
    }
    billed
}

#[test]
fn an_evaluation_keeps_each_answers_value_and_what_it_billed() {
    let details = json_fixture(
        r#"{
            "model": "jev-1",
            "answers": {
                "restart": {"noul": 0.96},
                "kind": {
                    "choice": "ops",
                    "confidence": 0.81,
                    "probabilities": {"ops": 0.93, "code": 0.07}
                },
                "risk": {
                    "score": 1.5,
                    "confidence": 0.5,
                    "probabilities": {"0": 0.0, "1": 0.5, "2": 0.5},
                    "legend": {"0": "low", "1": "medium", "2": "high"}
                }
            },
            "usage": {"calls": 1, "inputTokens": 572, "outputTokens": 30}
        }"#,
    );
    let summary = summary_of(&details);
    assert_eq!(summary["usage"], billed(&details["usage"]), "{summary}");
    assert_eq!(
        json!({"answers": summary["answers"]}),
        json_fixture(
            r#"{
                "answers": {
                    "restart": {"noul": 0.96},
                    "kind": {"choice": "ops", "probabilities": {"ops": 0.93}},
                    "risk": {"score": 1.5}
                }
            }"#
        )
    );
}

/// A combined answer keeps its lines and the row its coverage; several
/// requests stay counted.
#[test]
fn a_sectioned_evaluation_keeps_lines_coverage_and_its_request_count() {
    let details = json_fixture(
        r#"{
            "model": "jev-1",
            "answers": {
                "kind": {
                    "choice": "a",
                    "confidence": 0.75,
                    "probabilities": {"a": 0.75, "b": 0.25},
                    "lines": {"startLine": 21, "endLine": 30}
                }
            },
            "coverage": {"totalSections": 4, "answered": 4},
            "usage": {"calls": 5, "inputTokens": 400, "outputTokens": 20},
            "next": {
                "read": {
                    "kind": "related",
                    "tool": "localFetch",
                    "input": {"queries": [{"path": "a"}]}
                }
            }
        }"#,
    );
    let summary = summary_of(&details);
    // The sections disagreed: each label's share stays.
    assert_eq!(
        summary["answers"]["kind"],
        json_fixture(
            r#"{
                "choice": "a",
                "probabilities": {"a": 0.75, "b": 0.25},
                "lines": {"startLine": 21, "endLine": 30}
            }"#
        )
    );
    let mut agreed = details.clone();
    agreed["answers"]["kind"]["probabilities"] = json_fixture(r#"{"a": 1.0, "b": 0.0}"#);
    assert_eq!(
        summary_of(&agreed)["answers"]["kind"],
        json_fixture(
            r#"{"choice": "a", "probabilities": {"a": 1.0}, "lines": {"startLine": 21, "endLine": 30}}"#
        )
    );
    assert_eq!(summary["coverage"], details["coverage"]);
    assert_eq!(summary["usage"]["calls"], 5);
    assert_eq!(summary["next"], details["next"]);
}

#[test]
fn image_provenance_stays_only_as_the_size_a_box_maps_to() {
    let asset = json_fixture(
        r#"{
            "source": {"tool": "localFetch", "query": {"path": "shot.png"}},
            "sourceDigest": "ab",
            "format": "png",
            "bytes": 10,
            "width": 716,
            "height": 1075,
            "sourceWidth": 1432,
            "sourceHeight": 2150,
            "path": "/tmp/x.png",
            "expiresAt": "2026"
        }"#,
    );
    let boxed = json!({
        "answers":{"where":{"box":{"top":0.1,"bottom":0.5,"left":0.1,"right":0.4},"confidence":0.9}},
        "assets":{"shot":asset},
    });
    let summary = summary_of(&boxed);
    assert_eq!(
        summary["assets"]["shot"],
        json_fixture(
            r#"{
                "width": 716,
                "height": 1075,
                "sourceWidth": 1432,
                "sourceHeight": 2150
            }"#
        )
    );
    assert!(summary["answers"]["where"].get("confidence").is_none());
    let plain = json!({"answers":{"logo":{"noul":0.9}},"assets":{"shot":asset}});
    assert!(summary_of(&plain).get("assets").is_none());
}

#[test]
fn a_section_is_its_lines_and_answers() {
    let details = json_fixture(
        r#"{
            "model": "jev-1",
            "resources": {
                "mid": {
                    "status": "ready",
                    "sourceDigest": "d4",
                    "sections": [
                        {
                            "id": "s12",
                            "kind": "declarations",
                            "name": "f_376 … f_406 (31 declarations)",
                            "target": {
                                "bytes": [23784, 25751],
                                "lines": {"startLine": 1876, "endLine": 2027}
                            },
                            "context": [
                                {
                                    "bytes": [21784, 23784],
                                    "lines": {"startLine": 1717, "endLine": 1875}
                                }
                            ],
                            "answers": {
                                "p": {"status": "answered", "answer": {"noul": 0.96}},
                                "kind": {
                                    "status": "answered",
                                    "answer": {
                                        "choice": "ops",
                                        "confidence": 0.8,
                                        "probabilities": {"ops": 0.9}
                                    }
                                },
                                "late": {"status": "unassessed", "reason": "deadline"}
                            }
                        }
                    ],
                    "coverage": {"totalSections": 1, "answered": 2, "unassessed": 1}
                }
            },
            "usage": {
                "calls": 1,
                "inputTokens": 5,
                "knownUsageCalls": 1,
                "models": ["jev-1"],
                "outputTokens": 3,
                "unknownUsageCalls": 0
            }
        }"#,
    );
    let summary = summary_of(&details);
    let section = &summary["resources"]["mid"]["sections"][0];
    assert_eq!(
        section,
        &json_fixture(
            r#"{
                "target": {"lines": {"startLine": 1876, "endLine": 2027}},
                "answers": {
                    "p": {"noul": 0.96},
                    "kind": {"choice": "ops", "probabilities": {"ops": 0.9}},
                    "late": {"status": "unassessed", "reason": "deadline"}
                }
            }"#
        )
    );
    // The owner's bound: a summary section is at most 45% of the details one
    // (40% before a summary choice kept its chosen label's probability).
    let (short, long) = (
        section.to_string().len(),
        details["resources"]["mid"]["sections"][0].to_string().len(),
    );
    assert!(short * 100 <= long * 45, "{short} of {long}");
    assert_eq!(summary["usage"], billed(&details["usage"]));
    assert_eq!(
        summary["resources"]["mid"]["coverage"],
        details["resources"]["mid"]["coverage"]
    );
}

#[test]
fn a_summary_names_the_quiet_units_it_leaves_out() {
    let details = json_fixture(
        r#"{
            "units": {
                "a.rs:3": {"auth": {"status": "answered", "answer": {"noul": 0.91}}},
                "a.rs:9": {"auth": {"status": "answered", "answer": {"noul": 0.02}}},
                "b.rs:1": {"auth": {"status": "answered", "answer": {"noul": 0.1}}}
            },
            "coverage": {"totalUnits": 3, "answered": 3, "quiet": 2}
        }"#,
    );
    let summary = summary_of(&details);
    let warning = summary["warnings"][0].as_str().expect("a warning");
    assert!(
        warning.contains("a.rs:9") && warning.contains("b.rs:1"),
        "{warning}"
    );
    assert!(!warning.contains("a.rs:3"), "{warning}");
    // Listing them asks the provider again: the warning says it bills.
    assert!(warning.contains("billed"), "{warning}");
    // A row with no quiet unit says nothing.
    let loud = json_fixture(
        r#"{
            "units": {
                "a": {"auth": {"status": "answered", "answer": {"noul": 0.9}}}
            }
        }"#,
    );
    assert!(summary_of(&loud).get("warnings").is_none());
}

/// The quiet list names each file once, its lines ascending (flowD1: 51
/// keys of three files cut the summary from 2,184 to 1,262 chars); every
/// key stays recoverable, a second hit on one line and keys that name no
/// line included.
#[test]
fn the_quiet_list_names_each_file_once() {
    let keys = [
        "docs/handoffs.md:84",
        "docs/ref/handoff_filters.md:3",
        "docs/handoffs.md:7",
        "docs/handoffs.md:125#2",
        "docs/handoffs.md:125",
        "o/r",
        "42:1",
        "42:src/x.rs",
        "17",
    ]
    .map(str::to_owned);
    assert_eq!(
        super::named(&keys),
        "docs/handoffs.md:{7,84,125,125#2}, docs/ref/handoff_filters.md:3, o/r, 42:1, 42:src/x.rs, 17"
    );
}

/// Usage that says more than its spend (unreported requests, an estimate
/// of the rest) is kept whole.
#[test]
fn a_plans_usage_stays_whole_when_it_says_more_than_its_spend() {
    let details = json_fixture(
        r#"{
            "resources": {"a": {"status": "ready", "sections": []}},
            "usage": {
                "calls": 2,
                "inputTokens": 5,
                "knownUsageCalls": 1,
                "outputTokens": 3,
                "unknownUsageCalls": 1
            }
        }"#,
    );
    assert_eq!(summary_of(&details)["usage"], details["usage"]);
}

#[test]
fn a_sections_lead_longer_than_its_source_is_dropped() {
    let lead = json_fixture(
        r#"{
            "kind": "related",
            "tool": "clasify",
            "input": {
                "queries": [
                    {
                        "operation": "sections",
                        "questions": {"p": {"type": "noul", "instructions": "Is it planted?"}},
                        "resources": {
                            "source": {"tool": "localFetch", "query": {"path": "tiny.rs"}}
                        }
                    }
                ]
            }
        }"#,
    );
    let row = |read_chars: u64| json!({"answers":{"p":{"noul":0.9}},"readChars":read_chars,"related":[lead]});
    assert!(summary_of(&row(43)).get("related").is_none());
    assert_eq!(summary_of(&row(77_296))["related"], json!([lead]));
}

#[test]
fn quiet_units_leave_a_summary_and_stay_counted() {
    let details = json_fixture(
        r#"{
            "units": {
                "a.rs:3": {"auth": {"status": "answered", "answer": {"noul": 0.91}}},
                "a.rs:9": {"auth": {"status": "answered", "answer": {"noul": 0.02}}},
                "b.rs:1": {
                    "auth": {
                        "status": "error",
                        "error": {"errorCode": "timeout", "error": "late"}
                    }
                }
            },
            "coverage": {"totalUnits": 3, "answered": 2, "quiet": 1, "error": 1},
            "usage": {"calls": 3, "inputTokens": 9, "outputTokens": 3}
        }"#,
    );
    let summary = summary_of(&details);
    assert_eq!(
        summary["units"],
        json_fixture(
            r#"{
                "a.rs:3": {"auth": {"noul": 0.91}},
                "b.rs:1": {
                    "auth": {
                        "status": "error",
                        "error": {"errorCode": "timeout", "error": "late"}
                    }
                }
            }"#
        )
    );
    assert_eq!(summary["coverage"], details["coverage"]);
}

#[test]
fn a_failed_row_and_a_model_listing_are_left_whole() {
    for row in [
        json_fixture(
            r#"{
                "errorCode": "classificationProviderError",
                "error": "x",
                "usage": {"calls": 1, "inputTokens": 0, "outputTokens": 0}
            }"#,
        ),
        json_fixture(
            r#"{
                "models": [{"name": "jev", "description": "d", "releaseDate": "r"}]
            }"#,
        ),
    ] {
        assert_eq!(summary_of(&row), row);
    }
}

#[test]
fn a_details_continuation_stays_in_details() {
    let mut row = json_fixture(
        r#"{
            "next": {
                "nextPage": {
                    "kind": "continue",
                    "tool": "clasify",
                    "input": {"queries": [{"operation": "sections", "assessment": "t"}]}
                },
                "read": {
                    "kind": "continue",
                    "tool": "localFetch",
                    "input": {"queries": [{"path": "a"}]}
                }
            }
        }"#,
    );
    carry_details(&mut row);
    assert_eq!(
        row["next"]["nextPage"]["input"]["queries"][0]["view"],
        "details"
    );
    assert!(
        row["next"]["read"]["input"]["queries"][0]
            .get("view")
            .is_none()
    );
}

/// Each resource of a row bills its own request: every one states its
/// `calls`, a single request included, so the summary shows what the call
/// spent.
#[test]
fn every_resource_states_its_calls() {
    let resource = |noul: f64| {
        json!({
            "model": "jev-1",
            "answers": {"q": {"noul": noul}},
            "usage": {"calls": 1, "inputTokens": 90, "outputTokens": 4}
        })
    };
    let details = json!({"resources": {"a.md": resource(0.9), "b.md": resource(0.1)}});
    let summary = summary_of(&details);
    for name in ["a.md", "b.md"] {
        assert_eq!(
            summary["resources"][name]["usage"],
            billed(&details["resources"][name]["usage"]),
            "{summary}"
        );
    }
    let one = json!({"resources": {"a.md": resource(0.9)}});
    assert_eq!(
        summary_of(&one)["resources"]["a.md"]["usage"],
        billed(&one["resources"]["a.md"]["usage"])
    );
}

/// A summary of a row whose hits sit under their files: quiet hits leave
/// (named `path:line` in the warning), a file left empty leaves, answers
/// lose their distribution, and the plan that came true is the request
/// count itself.
#[test]
fn a_files_rows_summary_leaves_quiet_hits_out_by_path_and_line() {
    let details = json_fixture(
        r#"{
            "files": {
                "a.rs": {
                    "9": {"q": {"noul": 0.9}, "kind": {"choice": "a", "confidence": 0.8, "probabilities": {"a": 0.8, "b": 0.2}}},
                    "3": {"q": {"noul": 0.2}, "kind": {"choice": "b", "confidence": 0.6, "probabilities": {"a": 0.4, "b": 0.6}}}
                },
                "b.rs": {"1": {"q": {"noul": 0.1}}}
            },
            "units": {"o/r": {"q": {"status": "error", "error": {"errorCode": "timeout", "error": "late"}}}},
            "coverage": {"totalUnits": 4, "answered": 3, "quiet": 1, "error": 1},
            "usage": {"calls": 2, "requestsPlanned": 2, "requestsCeiling": 4, "inputTokens": 9, "outputTokens": 3}
        }"#,
    );
    let summary = summary_of(&details);
    assert_eq!(
        summary["files"],
        json_fixture(
            r#"{"a.rs": {
                "9": {"q": {"noul": 0.9}, "kind": {"choice": "a", "probabilities": {"a": 0.8}}},
                "3": {"q": {"noul": 0.2}, "kind": {"choice": "b", "probabilities": {"b": 0.6}}}
            }}"#
        )
    );
    assert_eq!(summary["units"], details["units"]);
    let warning = summary["warnings"][0].as_str().expect("a warning");
    assert!(
        warning.contains("b.rs:1") && !warning.contains("a.rs"),
        "{warning}"
    );
    assert_eq!(summary["usage"], billed(&details["usage"]));
    // Every hit quiet: the summary states no files.
    let quiet = json_fixture(
        r#"{"files": {"b.rs": {"1": {"q": {"noul": 0.1}}}}, "coverage": {"totalUnits": 1, "answered": 1, "quiet": 1}}"#,
    );
    assert!(summary_of(&quiet).get("files").is_none());
}

/// A source judged part by part: the summary keeps every part that answered
/// yes or undecided with its lines and its own answers, leaves the quiet
/// ones to `coverage.quiet`, and states nothing the details row lacks.
#[test]
fn a_sectioned_evaluation_keeps_its_loud_parts_with_their_lines() {
    let part = |start: u64, noul: f64| {
        json!({
            "target": {"bytes": [start * 10, start * 10 + 99], "lines": {"startLine": start, "endLine": start + 9}},
            "answers": {"q": {"status": "answered", "answer": {"type": "noul", "noul": noul}}}
        })
    };
    let details = json!({
        "answers": {"q": {"noul": 0.9, "lines": {"startLine": 11, "endLine": 20}}},
        "coverage": {"totalSections": 3, "answered": 3, "quiet": 2},
        "sections": [part(1, 0.1), part(11, 0.9), part(21, 0.2)],
        "usage": {"calls": 3, "inputTokens": 300, "outputTokens": 12}
    });
    let summary = summary_of(&details);
    let parts = summary["sections"].as_array().expect("the parts");
    let total = details["coverage"]["totalSections"].as_u64().unwrap();
    let quiet = summary["coverage"]["quiet"].as_u64().unwrap();
    assert_eq!(parts.len() as u64 + quiet, total, "{summary}");
    assert_eq!(
        parts[0]["target"],
        json!({"lines": {"startLine": 11, "endLine": 20}})
    );
    assert_eq!(parts[0]["answers"]["q"]["noul"], 0.9, "{summary}");
}

/// A summary choice keeps the chosen label's probability, what an agent
/// weighs acting on it by; the other labels' stay in details.
#[test]
fn a_summary_choice_keeps_its_labels_probability() {
    let details = json_fixture(
        r#"{
            "answers": {
                "edit": {
                    "choice": "rename",
                    "confidence": 0.4,
                    "probabilities": {"rename": 0.55, "history": 0.4, "example": 0.05}
                }
            },
            "usage": {"calls": 1, "inputTokens": 90, "outputTokens": 9}
        }"#,
    );
    assert_eq!(
        summary_of(&details)["answers"]["edit"],
        json!({"choice": "rename", "probabilities": {"rename": 0.55}})
    );
}
