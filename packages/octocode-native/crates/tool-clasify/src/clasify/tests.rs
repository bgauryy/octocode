use super::*;
use octocode_contract_types::ErrorCode;
use octocode_test_support::json_fixture;

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
        json_fixture(r#"{"status": "ready"}"#),
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
    octocode_runtime_base::contracts::validate_output(
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
    use octocode_tools_shared::judgment::Judged;
    let row = Row {
        context: None,
        goal: None,
        background: None,
        model: None,
        keys: vec!["q".into(), "kind".into()],
        questions: vec![
            json_fixture(r#"{"type": "noul", "instructions": "Is it set?"}"#),
            json_fixture(
                r#"{
                    "type": "choice",
                    "instructions": "Kind?",
                    "criteria": {"a": "A.", "b": "B."}
                }"#,
            ),
        ],
        input: Input::Models,
        assets: None,
        long_context: None,
        details: true,
        per_item: false,
        source_progress: None,
        max_requests: None,
        walk_requests: 0,
        saved_page: None,
        per_hit: false,
        collapse: false,
        retry_units: None,
        query: Value::Null,
    };
    let local = |path: &str| json!({"tool":"localFetch","query":{"path":path}});
    let missing = octocode_tools_shared::judgment::ClassificationError::new(
        ErrorCode::NotFound,
        "No file matches the glob 'gone/*.md'.",
        "Check the glob.",
    );
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let judge = |source: &Value| {
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
                    json_fixture(
                        r#"{
                                "type": "choice",
                                "choice": "a",
                                "confidence": 1,
                                "probabilities": {"a": 1, "b": 0}
                            }"#,
                    ),
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
            price: None,
        }
    };
    let entries: Vec<(String, Result<Value, _>)> = vec![
        ("one".into(), Ok(local("one.md"))),
        ("gone".into(), Err(missing)),
        ("two".into(), Ok(local("two.md"))),
    ];
    let outcomes = evaluate::each(entries, 2, |(key, entry)| match entry {
        Ok(source) => evaluate::ResourceOutcome {
            key,
            outcome: judge(&source),
            source: Some(source),
        },
        Err(error) => evaluate::ResourceOutcome {
            key,
            source: None,
            outcome: RowOutcome::Judgment {
                judged: Err(error),
                captured: None,
                assets: None,
                identity: None,
                price: None,
            },
        },
    });
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
        goal: None,
        background: None,
        model: None,
        keys: vec!["q".into()],
        questions: vec![json_fixture(
            r#"{"type": "noul", "instructions": "Is it set?"}"#,
        )],
        input: Input::Resources(resources),
        assets: None,
        long_context: None,
        details: true,
        per_item: false,
        source_progress: None,
        max_requests: None,
        walk_requests: 0,
        saved_page: None,
        per_hit: false,
        collapse: false,
        retry_units: None,
        query: Value::Null,
    }
}

fn local(path: &str) -> Value {
    json!({"tool":"localFetch","query":{"path":path}})
}

fn answered_by(model: &str, noul: f64) -> RowOutcome {
    RowOutcome::Judgment {
        judged: Ok(octocode_tools_shared::judgment::Judged {
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
        price: None,
    }
}

pub fn failed_with(error: octocode_tools_shared::judgment::ClassificationError) -> RowOutcome {
    RowOutcome::Judgment {
        judged: Err(error),
        captured: None,
        assets: None,
        identity: None,
        price: None,
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
    let mut error = octocode_tools_shared::judgment::ClassificationError::new(
        ErrorCode::ClassificationContextTooLarge,
        "The selected source exceeds the window.",
        "Judge it in sections.",
    );
    error.provider_calls = 2;
    let retry = json_fixture(
        r#"{
            "tool": "clasify",
            "kind": "continue",
            "input": {"queries": [{"operation": "models"}]}
        }"#,
    );
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
        failed_with(octocode_tools_shared::judgment::ClassificationError::new(
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
        let yes = [json_fixture(r#"{"type": "noul", "noul": 0.9}"#)];
        for (key, path) in keyed {
            across.judged(0, &row, Some(&local(path)), Some(key), &yes);
        }
        let mut rows = vec![json_fixture(r#"{"resources": {}}"#)];
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
    let stopped = octocode_tools_shared::judgment::ClassificationError::new(
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
        octocode_tools_shared::judgment::stats::ClassificationUsage::default().output(),
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
    assert_eq!(
        out["answers"]["q"],
        json_fixture(r#"{"noul": 0.9}"#),
        "{out}"
    );
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
        judged.long_context_retry = Some(octocode_tools_shared::judgment::LongContextRetry {
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

/// A row whose requests, before any transient retry, are over its
/// `maxRequests` sends nothing: it states the price (no request sent, the
/// first round, the ceiling with its shared retries, the input tokens at
/// most) and offers the same row at the ceiling as `next.confirm`.
#[test]
fn an_over_budget_row_states_its_price_and_offers_the_confirm_call() {
    let rows = super::rows::parse_rows(&[json!({
        "operation": "evaluate",
        "source": {"tool": "ghSearchCode", "query": {"owner": "o", "repo": "r", "keywords": ["x"]}},
        "questions": {"q": {"type": "noul", "instructions": "Is it?"}},
        "maxRequests": 2
    })]);
    let over = reply::OverBudget {
        units: 63,
        price: reply::Price {
            planned: 3,
            ceiling: Some(6),
            room: 3,
            tokens: Some(21_400),
        },
        limit: reply::Limit::Max(2),
        room: 3,
        spent: 0,
    };
    let out = reply::over_budget(&rows[0], &over, true);
    assert_eq!(out["errorCode"], "classificationOverBudget", "{out:#}");
    assert_eq!(
        out["usage"],
        json!({"calls":0,"requestsPlanned":3,"requestsCeiling":6,"inputTokensAtMost":21400})
    );
    let error = out["error"].as_str().expect("error");
    // One number: the price the text states is the usage's ceiling.
    let ceiling = out["usage"]["requestsCeiling"].as_u64().expect("ceiling");
    assert!(
        error.contains("63 units")
            && error.contains(&format!("at most {ceiling} "))
            && error.contains("maxRequests is 2"),
        "{error}"
    );
    let hint = out["hints"][0].as_str().expect("hint");
    assert!(
        hint.contains("at its ceiling, 6") && hint.contains("the 3 transient retries it shares"),
        "{hint}"
    );
    let confirm = &out["next"]["confirm"];
    assert_eq!(confirm["tool"], "clasify", "{out:#}");
    let query = &confirm["input"]["queries"][0];
    assert_eq!(query["maxRequests"], 6, "{out:#}");
    assert_eq!(query["source"], rows[0].query["source"], "{out:#}");
    assert!(out.get("warnings").is_none(), "{out:#}");
    // A runtime that keeps nothing offers it as a related call.
    let fresh = reply::over_budget(&rows[0], &over, false);
    assert!(fresh["next"].get("confirm").is_none(), "{fresh:#}");
    assert_eq!(fresh["related"][0]["input"]["queries"][0]["maxRequests"], 6);
}

/// A price is stated in the usage of a row that sent a request.
#[test]
fn a_rows_price_is_stated_in_its_usage() {
    let mut row = json!({"usage": {"calls": 1}});
    let price = reply::Price {
        planned: 1,
        ceiling: Some(3),
        room: 0,
        tokens: Some(900),
    };
    reply::priced(&mut row, Some(price));
    assert_eq!(
        row["usage"],
        json!({"calls":1,"requestsPlanned":1,"requestsCeiling":3})
    );
    let mut unsent = json!({"errorCode": "notFound"});
    reply::priced(&mut unsent, Some(price));
    assert!(unsent.get("usage").is_none());
}

/// A row is admitted by its first round: with `maxRequests`, when that
/// round fits the cap, held to it (its stated ceiling never passes the cap,
/// and the transient retries take only what the plan leaves); without it,
/// when that round sends no more requests than the row names. A plan past
/// its bound sends nothing and states its whole ceiling for `next.confirm`.
#[test]
fn a_row_is_admitted_by_its_first_round_within_its_cap_or_what_it_names() {
    let retries = 3;
    for first in 1..=4_u64 {
        for at_most in first..=8 {
            let plan = super::admission::Plan {
                first,
                at_most,
                bounded: true,
                units: 1,
                tokens: 0,
            };
            let full = octocode_tools_shared::judgment::RequestCap::ceiling(at_most, retries);
            for max in 1..=12_u64 {
                match super::admission::admit(Some(max), plan, 0, retries) {
                    Ok(admitted) => {
                        assert!(first <= max, "{first} past {max} admitted");
                        let ceiling = admitted.price.ceiling.expect("a capped row states one");
                        assert!(ceiling <= max, "{plan:?} in {max}: ceiling {ceiling}");
                        assert!(ceiling >= first, "its first round fits: {ceiling}");
                        assert_eq!(ceiling, full.min(max), "{first}/{at_most} in {max}");
                    }
                    Err(over) => {
                        assert!(first > max, "{first} within {max} refused");
                        assert_eq!(over.limit, reply::Limit::Max(max));
                        assert_eq!(over.price.ceiling, Some(full));
                    }
                }
            }
            for named in 0..=5_u64 {
                match super::admission::admit(None, plan, named, retries) {
                    Ok(admitted) => {
                        assert!(first <= named, "{first} past {named} named admitted");
                        assert_eq!(admitted.price.ceiling, Some(full));
                    }
                    Err(over) => {
                        assert!(first > named, "{first} within {named} named refused");
                        assert_eq!(over.limit, reply::Limit::Named(named));
                        assert_eq!(over.price.ceiling, Some(full));
                    }
                }
            }
        }
    }
}

/// Every refusal a bound makes is a row the clasify output contract takes,
/// with its `next.confirm`: an evaluate row over its `maxRequests` or past
/// what it names, a `sections` plan past its resources, a source the window
/// refused whole whose sections wait, and one resource of a resources row.
#[test]
fn every_bound_refusal_is_a_row_the_output_contract_takes() {
    use octocode_runtime_base::id::ToolId;
    use octocode_runtime_base::test_support::assert_row_contract;
    let questions = json!({"q":{"type":"noul","instructions":"Is it?"}});
    let local = |path: &str| json!({"tool":"localFetch","query":{"path":path}});
    let row = |query: Value| super::rows::parse_rows(&[query]).remove(0);
    let search = row(json!({"operation":"evaluate","questions":questions,
        "source":{"tool":"localSearch","query":{"path":".","matchString":["x"]}}}));
    let whole = row(json!({"operation":"evaluate","questions":questions,"source":local("big.md")}));
    let sections = row(json!({"operation":"sections","questions":questions,
        "resources":{"docs":local("docs/a.md")}}));
    let over = |limit, spent| reply::OverBudget {
        units: 3,
        price: reply::Price {
            planned: 3,
            ceiling: Some(7),
            room: 3,
            tokens: Some(0),
        },
        limit,
        room: 3,
        spent,
    };
    for (row, over) in [
        (&search, over(reply::Limit::Max(2), 0)),
        (&search, over(reply::Limit::Named(1), 0)),
        (&sections, over(reply::Limit::Named(1), 0)),
        (&whole, over(reply::Limit::Named(1), 1)),
    ] {
        for can_resume in [true, false] {
            let out = reply::over_budget(row, &over, can_resume);
            assert_eq!(out["errorCode"], "classificationOverBudget", "{out:#}");
            assert_eq!(out["usage"]["calls"], over.spent, "{out:#}");
            let confirm = if can_resume {
                &out["next"]["confirm"]
            } else {
                &out["related"][0]
            };
            assert_eq!(confirm["tool"], "clasify", "{out:#}");
            assert_row_contract(ToolId::Clasify, &out);
        }
    }
    let resources = row(json!({"operation":"evaluate","questions":questions,
        "resources":{"big":local("big.md")}}));
    let outcomes = vec![evaluate::ResourceOutcome {
        key: "big".into(),
        source: Some(local("big.md")),
        outcome: RowOutcome::OverBudget(Box::new(over(reply::Limit::Named(1), 1))),
    }];
    let out = reply::resources_answered(&resources, outcomes, Vec::new(), true);
    assert_eq!(
        out["resources"]["big"]["errorCode"], "classificationOverBudget",
        "{out:#}"
    );
    assert_eq!(
        out["resources"]["big"]["next"]["confirm"]["tool"], "clasify",
        "{out:#}"
    );
    assert_row_contract(ToolId::Clasify, &out);
}
