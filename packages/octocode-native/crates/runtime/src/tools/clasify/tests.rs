use super::*;
use octocode_contract_types::ErrorCode;

/// A sections row fails only when no resource was read: then as its
/// reads did, one code, else a failed context.
#[test]
fn a_sections_row_with_no_resource_read_fails_like_its_reads() {
    let resource =
        |status: &str, code: &str| json!({"status":status,"error":{"errorCode":code,"error":"x"}});
    let row = |resources: Value| json!({"operation":"sections","resources":resources});
    let missing = resource("error", "notFound");
    assert_eq!(
        unread_resources_code(&row(json!({"a":missing,"b":missing}))),
        Some(ErrorCode::NotFound)
    );
    assert_eq!(
        unread_resources_code(&row(
            json!({"a":missing,"b":resource("error","outsideAllowedRoots")})
        )),
        Some(ErrorCode::ClassificationContextFailed)
    );
    for other in [
        json!({"status":"ready"}),
        resource("unsupported", "classificationContextTooLarge"),
    ] {
        assert_eq!(
            unread_resources_code(&row(json!({"a":missing,"b":other}))),
            None
        );
    }
    assert_eq!(unread_resources_code(&row(json!({}))), None);
}

/// `models` marks the entry a row without `model` is judged by, and
/// nothing when the listing does not name it.
#[test]
fn models_mark_the_default_model_only_when_listed() {
    let model = |name: &str, release_date: &str| octocode_contract_types::ClasifyModelMetadata {
        default: None,
        description: "d".into(),
        name: name.into(),
        release_date: release_date.into(),
    };
    let listing =
        json!({"models":[model("jev-1.13", "2026-01-01"), model("jev-1.14", "2026-06-01")]});
    let mut marked = listing.clone();
    super::mark_default(&mut marked, "jev-1.14");
    assert!(marked["models"][0].get("default").is_none(), "{marked}");
    assert_eq!(marked["models"][1]["default"], true, "{marked}");
    crate::contracts::validate_output(
        "clasify",
        &json!({"results":[{"index":0,"data":{"models":marked["models"]}}]}),
    )
    .unwrap_or_else(|error| panic!("models row: {error:?}"));
    let mut unknown = listing.clone();
    super::mark_default(&mut unknown, "jev-preview");
    assert_eq!(unknown, listing);
}

/// An `evaluate` row with `resources` judges each resource whole, one
/// request per resource with every question; a resource that failed keeps
/// its own error beside the others' answers.
#[test]
fn each_resource_is_judged_whole_and_a_failure_stays_with_it() {
    use crate::tools::judgment::Judged;
    let row = Row {
        context: None,
        model: None,
        keys: vec!["q".into(), "kind".into()],
        questions: vec![
            json!({"type":"noul","instructions":"Is it set?"}),
            json!({"type":"choice","instructions":"Kind?","criteria":{"a":"A.","b":"B."}}),
        ],
        input: Input::Models,
        assets: None,
        long_context: None,
        details: true,
        per_item: false,
    };
    let local = |path: &str| json!({"tool":"localFetch","query":{"path":path}});
    let missing = crate::tools::judgment::ClassificationError::new(
        ErrorCode::NotFound,
        "No file matches the glob 'gone/*.md'.",
        "Check the glob.",
    );
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let outcomes = evaluate::each_resource(
        vec![
            ("one".into(), Ok(local("one.md"))),
            ("gone".into(), Err(missing)),
            ("two".into(), Ok(local("two.md"))),
        ],
        2,
        |source| {
            calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let yes = if source["query"]["path"] == "one.md" {
                0.9
            } else {
                0.2
            };
            RowOutcome::Judgment {
                judged: Ok(Judged {
                    answers: vec![
                        json!({"type":"noul","noul":yes}),
                        json!({"type":"choice","choice":"a","confidence":1,"probabilities":{"a":1,"b":0}}),
                    ],
                    model: "jev-1.13.0".into(),
                    input_tokens: 40,
                    output_tokens: 7,
                    provider_calls: 1,
                    known_usage_calls: 1,
                    long_context_retry: None,
                }),
                captured: None,
                assets: None,
                identity: None,
            }
        },
    );
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 2);
    let out = reply::resources_answered(&row, outcomes, Vec::new(), true);
    let resources = out["resources"].as_object().expect("resources");
    // In input order: a glob's files stay in walk order.
    let keys = resources.keys().map(String::as_str).collect::<Vec<_>>();
    assert_eq!(keys, ["one", "gone", "two"], "{out:#}");
    assert_eq!(out["model"], "jev-1.13.0", "{out:#}");
    assert_eq!(resources["one"]["answers"]["q"]["noul"], 0.9, "{out:#}");
    assert_eq!(
        resources["two"]["answers"]["kind"]["choice"], "a",
        "{out:#}"
    );
    for key in ["one", "two"] {
        assert_eq!(resources[key]["usage"]["calls"], 1, "{out:#}");
    }
    assert_eq!(resources["gone"]["errorCode"], "notFound", "{out:#}");
    assert!(resources["gone"].get("answers").is_none(), "{out:#}");
}

/// A row over `resources` with one noul question.
fn resources_row(resources: Value) -> Row {
    Row {
        context: None,
        model: None,
        keys: vec!["q".into()],
        questions: vec![json!({"type":"noul","instructions":"Is it set?"})],
        input: Input::Resources(resources),
        assets: None,
        long_context: None,
        details: true,
        per_item: false,
    }
}

fn local(path: &str) -> Value {
    json!({"tool":"localFetch","query":{"path":path}})
}

fn answered_by(model: &str, noul: f64) -> RowOutcome {
    RowOutcome::Judgment {
        judged: Ok(crate::tools::judgment::Judged {
            answers: vec![json!({"type":"noul","noul":noul})],
            model: model.into(),
            input_tokens: 40,
            output_tokens: 7,
            provider_calls: 1,
            known_usage_calls: 1,
            long_context_retry: None,
        }),
        captured: None,
        assets: None,
        identity: None,
    }
}

fn failed_with(error: crate::tools::judgment::ClassificationError) -> RowOutcome {
    RowOutcome::Judgment {
        judged: Err(error),
        captured: None,
        assets: None,
        identity: None,
    }
}

fn resource(key: &str, source: Value, outcome: RowOutcome) -> evaluate::ResourceOutcome {
    evaluate::ResourceOutcome {
        key: key.into(),
        source: Some(source),
        outcome,
    }
}

/// A failed resource keeps what the same failure keeps on a
/// `source` row: the requests it spent, its leads, and its recovery (a
/// too-large source offers the sections call).
#[test]
fn a_failed_resource_keeps_its_spend_leads_and_recovery() {
    let row = resources_row(json!({"big":local("big.md"),"ok":local("ok.md")}));
    let mut error = crate::tools::judgment::ClassificationError::new(
        ErrorCode::ClassificationContextTooLarge,
        "The selected source exceeds the window.",
        "Judge it in sections.",
    );
    error.provider_calls = 2;
    let retry =
        json!({"tool":"clasify","kind":"continue","input":{"queries":[{"operation":"models"}]}});
    error.leads = vec![("retry".into(), retry.clone())];
    let out = reply::resources_answered(
        &row,
        vec![
            resource("big", local("big.md"), failed_with(error)),
            resource("ok", local("ok.md"), answered_by("jev-1.13.0", 0.2)),
        ],
        Vec::new(),
        true,
    );
    let big = &out["resources"]["big"];
    assert_eq!(big["errorCode"], "classificationContextTooLarge", "{out:#}");
    assert_eq!(big["usage"]["calls"], 2, "the spent requests: {out:#}");
    assert_eq!(big["next"]["retry"], retry, "{out:#}");
    let split = out["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| call["input"]["queries"][0]["operation"] == "sections")
        .unwrap_or_else(|| panic!("the sections recovery: {out:#}"));
    assert_eq!(
        split["input"]["queries"][0]["resources"]["source"],
        local("big.md"),
        "{out:#}"
    );
}

/// A resources row whose every resource failed alike fails as
/// they did; one answered resource keeps the row a success.
#[test]
fn a_resources_row_whose_every_resource_failed_fails_like_them() {
    let row = resources_row(json!({"a":local("a.md"),"b":local("b.md")}));
    let refused = || {
        failed_with(crate::tools::judgment::ClassificationError::new(
            ErrorCode::ClassificationProviderError,
            "The provider refused.",
            "Retry later.",
        ))
    };
    let out = reply::resources_answered(
        &row,
        vec![
            resource("a", local("a.md"), refused()),
            resource("b", local("b.md"), refused()),
        ],
        Vec::new(),
        true,
    );
    assert_eq!(out["errorCode"], "classificationProviderError", "{out:#}");
    assert!(out["error"].is_string(), "{out:#}");
    let mixed = reply::resources_answered(
        &row,
        vec![
            resource("a", local("a.md"), refused()),
            resource("b", local("b.md"), answered_by("jev-1.13.0", 0.2)),
        ],
        Vec::new(),
        true,
    );
    assert!(mixed.get("errorCode").is_none(), "{mixed:#}");
}

/// A resource answered by another model than the row's says so.
#[test]
fn a_resource_answered_by_another_model_names_it() {
    let row = resources_row(json!({"a":local("a.md"),"b":local("b.md")}));
    let out = reply::resources_answered(
        &row,
        vec![
            resource("a", local("a.md"), answered_by("jev-1.13.0", 0.2)),
            resource("b", local("b.md"), answered_by("jev-1.14.0", 0.2)),
        ],
        Vec::new(),
        true,
    );
    assert_eq!(out["model"], "jev-1.13.0", "{out:#}");
    assert!(out["resources"]["a"].get("model").is_none(), "{out:#}");
    assert_eq!(out["resources"]["b"]["model"], "jev-1.14.0", "{out:#}");
}

/// The sections lead over a resources row names its resources by
/// the keys the row shows, a glob's `<key>/<path>` included.
#[test]
fn the_sections_lead_keeps_the_rows_resource_keys() {
    let lead_over = |keyed: [(&str, &str); 2]| {
        let row = resources_row(json!({}));
        let mut across = across::Across::default();
        let yes = [json!({"type":"noul","noul":0.9})];
        for (key, path) in keyed {
            across.judged(0, &row, Some(&local(path)), Some(key), &yes);
        }
        let mut rows = vec![json!({"resources":{}})];
        across.attach(&mut rows, std::slice::from_ref(&row), true);
        rows[0]["related"][0]["input"]["queries"][0]["resources"].clone()
    };
    assert_eq!(
        lead_over([("a.md", "docs/a.md"), ("b.md", "docs/b.md")]),
        json!({"a.md":local("docs/a.md"),"b.md":local("docs/b.md")})
    );
    assert_eq!(
        lead_over([("a.md", "docs/a.md"), ("d/b.md", "docs/b.md")]),
        json!({"a.md":local("docs/a.md"),"d/b.md":local("docs/b.md")})
    );
}

/// A glob the call stopped before it expanded offers no read
/// of the glob path, which localFetch would read literally and miss; its way
/// back is the same row over that glob.
#[test]
fn an_unexpanded_glob_offers_no_read_of_its_pattern() {
    let glob = local("docs/*.md");
    let row = resources_row(json!({"g":glob}));
    let stopped = crate::tools::judgment::ClassificationError::new(
        ErrorCode::Cancelled,
        "The call stopped before its source was captured.",
        "Retry.",
    );
    let out = reply::resources_answered(
        &row,
        vec![evaluate::ResourceOutcome {
            key: "g".into(),
            source: None,
            outcome: failed_with(stopped),
        }],
        Vec::new(),
        true,
    );
    assert_eq!(out["resources"]["g"]["errorCode"], "cancelled", "{out:#}");
    let reads_the_pattern = |call: &Value| {
        call["tool"] == "localFetch" && call["input"]["queries"][0]["path"] == "docs/*.md"
    };
    let offered = out["related"].as_array().into_iter().flatten().chain(
        out["resources"]["g"]["next"]
            .as_object()
            .into_iter()
            .flat_map(|next| next.values()),
    );
    assert!(!offered.into_iter().any(reads_the_pattern), "{out:#}");
    let rerun = out["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| call["tool"] == "clasify")
        .unwrap_or_else(|| panic!("the same row over the glob: {out:#}"));
    let query = &rerun["input"]["queries"][0];
    assert_eq!(query["operation"], "evaluate", "{rerun:#}");
    assert_eq!(query["resources"], json!({"g":glob}), "{rerun:#}");
    assert_eq!(query["questions"]["q"], row.questions[0], "{rerun:#}");
}

/// Evaluate spells its token fields as sections and every other
/// tool's usage does.
#[test]
fn evaluate_usage_spells_tokens_like_selection_usage() {
    let evaluate = super::reply::evaluation_usage(1, 1, 1, 0);
    let sections = serde_json::to_value(
        crate::tools::judgment::stats::ClassificationUsage::default().output(),
    )
    .expect("usage");
    for key in evaluate.as_object().expect("object").keys() {
        assert!(
            sections.get(key).is_some(),
            "{key}: evaluate {evaluate} vs sections {sections}"
        );
    }
}

/// An evaluate answer names its type once, by its value key, as a sections
/// answer does.
#[test]
fn evaluate_answers_name_their_type_by_key_only() {
    let row = resources_row(json!({"a":local("a.rs")}));
    let RowOutcome::Judgment {
        judged: Ok(judged), ..
    } = answered_by("jev-1.14.0", 0.9)
    else {
        unreachable!()
    };
    let out = reply::answered(&row, judged, None, None);
    assert_eq!(out["answers"]["q"], json!({"noul":0.9}), "{out}");
}

#[test]
fn classification_disclosures_keep_resource_coverage_and_failed_resource_recovery() {
    let row = resources_row(json!({"ok":local("ok.rs"),"bad":local("bad.rs")}));
    let error = ClassificationError::new(
        ErrorCode::ClassificationProviderError,
        "The provider refused.",
        "Check the provider response.",
    );
    let notice = "One hidden path was withheld from the resource expansion.";
    let mut ok = answered_by("jev-1.14.0", 0.9);
    if let RowOutcome::Judgment {
        judged: Ok(judged), ..
    } = &mut ok
    {
        judged.long_context_retry = Some(crate::tools::judgment::LongContextRetry {
            refused_calls: 1,
            unreported_calls: 0,
        });
    }
    let output = reply::resources_answered(
        &row,
        vec![
            resource("ok", local("ok.rs"), ok),
            resource("bad", local("bad.rs"), failed_with(error)),
        ],
        vec![notice.into()],
        true,
    );
    assert_eq!(output["warnings"], json!([notice]), "{output}");
    assert!(output.get("hints").is_none(), "{output}");
    assert!(
        output["resources"]["ok"]["warnings"][0]
            .as_str()
            .is_some_and(|warning| warning.contains("40 input tokens billed")),
        "{output}"
    );
    assert!(
        output["resources"]["bad"]["hints"]
            .as_array()
            .expect("failed resource recovery")
            .iter()
            .any(|hint| hint == "Check the provider response."),
        "{output}"
    );
}
