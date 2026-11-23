//! The `repository` row against a fake GitHub at the external boundary.
use super::*;
use crate::artifact::test_http::{RouteHttp, test_budget};
use crate::security_check::github_double::Double;
use octocode_runtime_base::providers::saved_answers::ReusedSince;
use octocode_test_support::json_fixture;

const NOW: i64 = 1_800_000_000_000;

fn contract() -> &'static SecurityContract {
    security_contract().expect("security block")
}

fn facts(stars: u64, days_since_push: u64, archived: bool) -> RepoFacts {
    let pushed = NOW - i64::try_from(days_since_push).expect("days") * DAY_MILLIS;
    RepoFacts {
        owner: "o".into(),
        name: "r".into(),
        stars: usize::try_from(stars).expect("stars"),
        pushed_at: Some(octocode_runtime_base::civil_date::iso8601_millis(pushed)),
        archived,
        ..RepoFacts::default()
    }
}

fn rules(read: &RepoRead) -> Vec<String> {
    cautions(contract(), read, NOW, "today")
        .iter()
        .filter_map(|c| c["rule"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn thresholds_fire_one_past_the_contract_values_and_not_at_them() {
    let policy = &contract().policy;
    let at = RepoRead::Found(facts(
        policy.low_traction_stars,
        policy.unmaintained_after_days,
        false,
    ));
    assert!(rules(&at).is_empty(), "{:?}", rules(&at));
    let past = RepoRead::Found(facts(
        policy.low_traction_stars - 1,
        policy.unmaintained_after_days + 1,
        false,
    ));
    assert_eq!(rules(&past), ["unmaintained", "lowTraction"]);
}

#[test]
fn every_caution_carries_the_contract_s_level_and_basis() {
    let policy = &contract().policy;
    let reads = [
        RepoRead::Found(facts(0, policy.unmaintained_after_days + 1, true)),
        RepoRead::Missing,
        RepoRead::Moved(facts(policy.low_traction_stars, 0, false)),
    ];
    let mut fired = Vec::new();
    for read in &reads {
        fired.extend(cautions(contract(), read, NOW, "today"));
    }
    let names: Vec<_> = fired.iter().filter_map(|c| c["rule"].as_str()).collect();
    for expected in [
        "archived",
        "unmaintained",
        "lowTraction",
        "repositoryMissing",
        "repositoryMoved",
    ] {
        assert!(names.contains(&expected), "{expected}: {names:?}");
    }
    for caution in &fired {
        let rule = contract()
            .rule(caution["rule"].as_str().expect("rule"))
            .expect("a contract rule");
        let level = serde_json::to_value(match rule.level {
            CautionLevel::Review => "review",
            CautionLevel::Note => "note",
        })
        .expect("level");
        assert_eq!(caution["level"], level, "{caution}");
    }
}

#[test]
fn unknown_data_draws_no_caution() {
    assert!(
        rules(&RepoRead::Failed(RepoFailure {
            code: ErrorCode::RateLimited,
            why: "x".into()
        }))
        .is_empty()
    );
    let mut unpushed = facts(contract().policy.low_traction_stars, 0, false);
    unpushed.pushed_at = None;
    assert!(rules(&RepoRead::Found(unpushed)).is_empty());
}

async fn row(read: RepoRead, repo: &str) -> Result<Value, ArtifactError> {
    let http = RouteHttp::new();
    let budget = test_budget();
    let reused = ReusedSince::default();
    let fake = Double::repo(read).page(None, Vec::new(), None);
    let config = crate::security_check::tests::default_config();
    run(
        repo.split_once('/').expect("owner/name"),
        None,
        serde_json::Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
        &Net {
            http: &http,
            budget: &budget,
            answers: None,
            reused: &reused,
            config: &config,
            github: Some(&fake),
            osv_api: octocode_runtime_base::execution::RuntimeLimits::DEFAULT.osv_api,
            share: &crate::artifact::CallShare::new(),
        },
    )
    .await
}

#[tokio::test]
async fn a_failed_read_is_partial_with_source_failed_never_clean() {
    let failure = RepoFailure {
        code: ErrorCode::RateLimited,
        why: "quota".into(),
    };
    let data = row(RepoRead::Failed(failure), "o/r").await.expect("row");
    assert_eq!(data["sources"][0]["status"], "failed");
    assert_eq!(data["sources"][0]["errorCode"], "rateLimited");
    assert_eq!(data["isPartial"], true);
    assert!(
        data["partialReasons"].to_string().contains("sourceFailed"),
        "{data}"
    );
    assert!(data.get("repository").is_none() && data.get("cautions").is_none());
}

#[tokio::test]
async fn a_found_repository_states_facts_policy_and_the_day_read() {
    let policy = &contract().policy;
    let read = RepoRead::Found(facts(policy.low_traction_stars - 1, 1, false));
    let data = row(read, "https://github.com/o/r").await.expect("row");
    assert_eq!(data["repository"]["owner"], "o");
    assert!(data["repository"]["readOn"].is_string());
    assert!(data.get("policy").is_none(), "{data}");
    assert_eq!(data["cautions"][0]["rule"], "lowTraction");
    // Described, with no advisory: none known, not "found".
    assert_eq!(data["sources"][0]["status"], "noneKnown");
}

#[test]
fn every_form_of_a_repository_names_the_same_owner_and_name() {
    for text in [
        "meh/dux",
        " meh/dux ",
        "https://github.com/meh/dux",
        "http://github.com/meh/dux/",
        "github.com/meh/dux.git",
        "https://www.github.com/meh/dux",
        "https://github.com/meh/dux?tab=readme",
        "https://github.com/meh/dux#readme",
        "https://github.com/meh/dux.git?a=1#b",
        "git@github.com:meh/dux",
        "git@github.com:meh/dux.git",
        "ssh://git@github.com/meh/dux.git",
    ] {
        assert_eq!(
            parse(text).expect(text),
            ("meh".to_owned(), "dux".to_owned()),
            "{text}"
        );
    }
    assert_eq!(
        parse("a-b/c.d_e-f").expect("charset"),
        ("a-b".to_owned(), "c.d_e-f".to_owned())
    );
    assert_eq!(
        parse("o/.github").expect("dot-led name"),
        ("o".to_owned(), ".github".to_owned())
    );
}

#[test]
fn text_that_names_no_github_repository_is_invalid_input_never_a_missing_one() {
    let long_owner = format!("{}/r", "o".repeat(40));
    let long_name = format!("o/{}", "r".repeat(101));
    for text in [
        "",
        "justaname",
        "a/b/c",
        "/r",
        "o/",
        "o/.git",
        "../user",
        "o/..",
        "o/.",
        "o?x/r",
        "o/r?x",
        "o/r#x",
        "o/r s",
        "o /r",
        "ünï/r",
        "https://github.com/o",
        "https://github.com/",
        "https://github.com/o/r/tree/main",
        "https://gitlab.com/o/r",
        "git@gitlab.com:o/r",
        "git@github.com:o",
        "https://github.com/../r",
        "https://github.com/o/..?x",
        long_owner.as_str(),
        long_name.as_str(),
    ] {
        let error = parse(text).expect_err(text);
        assert_eq!(error.code, ErrorCode::InvalidInput, "{text}");
    }
}

#[tokio::test]
async fn a_github_transport_that_could_not_be_built_says_why_on_the_row() {
    let http = RouteHttp::new();
    let budget = test_budget();
    let reused = ReusedSince::default();
    let config = crate::security_check::tests::default_config();
    let failed = FailedRepository("transport broke".into());
    let data = run(
        ("o", "r"),
        None,
        serde_json::Map::new(),
        octocode_contract_types::SecurityCheckView::Details,
        &Net {
            http: &http,
            budget: &budget,
            answers: None,
            reused: &reused,
            config: &config,
            github: Some(&failed),
            osv_api: octocode_runtime_base::execution::RuntimeLimits::DEFAULT.osv_api,
            share: &crate::artifact::CallShare::new(),
        },
    )
    .await
    .expect("row");
    assert_eq!(data["sources"][0]["status"], "failed");
    assert_eq!(data["sources"][0]["why"], "transport broke");
    assert_eq!(data["isPartial"], true);
}

#[tokio::test]
async fn a_missing_repository_may_be_a_private_one() {
    let data = row(RepoRead::Missing, "o/r").await.expect("row");
    let said = data.to_string();
    assert!(said.contains("private"), "{said}");
    assert_eq!(data["cautions"][0]["rule"], "repositoryMissing");
}

/// Enterprise Managed User logins are `handle_shortcode`: the underscore is part
/// of a real owner.
#[test]
fn an_enterprise_managed_user_owner_is_a_repository_owner() {
    for text in [
        "octocat_acme/dux",
        "https://github.com/octocat_acme/dux",
        "git@github.com:octocat_acme/dux.git",
    ] {
        assert_eq!(
            parse(text).expect(text),
            ("octocat_acme".to_owned(), "dux".to_owned()),
            "{text}"
        );
    }
}

fn named(owner: &str, name: &str) -> RepoFacts {
    RepoFacts {
        owner: owner.into(),
        name: name.into(),
        ..facts(1_000, 1, false)
    }
}

/// GitHub resolves owner and name without regard to case; the row states them
/// as it spells them, as `repository.owner` and `repository.name` do.
#[tokio::test]
async fn the_answer_names_the_repository_as_github_spells_it() {
    let data = row(
        RepoRead::Found(named("OctoCat", "Hello-World")),
        "octocat/hello-world",
    )
    .await
    .expect("row");
    let answer = data["answer"].as_str().expect("answer");
    assert!(answer.starts_with("OctoCat/Hello-World:"), "{answer}");
    assert_eq!(data["repository"]["owner"], "OctoCat");
    assert!(!answer.contains("octocat/hello-world"), "{answer}");
}

/// A repository that moved is named as asked and as it is now.
#[tokio::test]
async fn a_moved_repository_is_named_as_asked_and_as_it_is_now() {
    let data = row(RepoRead::Moved(named("helm", "helm")), "kubernetes/helm")
        .await
        .expect("row");
    let answer = data["answer"].as_str().expect("answer");
    assert!(
        answer.contains("kubernetes/helm") && answer.contains("helm/helm"),
        "{answer}"
    );
}

/// One repository row from `query`, against `github`.
async fn query_row(mut query: Value, github: &Double) -> Value {
    // The whole row, unless the test states a view.
    if query.get("view").is_none() {
        query["view"] = json!("details");
    }
    let http = RouteHttp::new();
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::security_check::tests::default_config();
    let query: octocode_contract_types::SecurityCheckQuery =
        serde_json::from_value(query).expect("query");
    crate::security_check::run(
        &query,
        &crate::security_check::github_double::net(&http, &budget, &reused, &config, github),
    )
    .await
    .expect("row")
}

fn ids(data: &Value) -> Vec<String> {
    data["advisories"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row["id"].as_str().map(str::to_owned))
        .collect()
}

/// A walk that stops after a page reads on from GitHub's cursor: the next page
/// asks only the pages not read, and the two pages list every advisory once.
#[tokio::test]
async fn an_advisory_walk_that_stops_reads_on_from_its_cursor_with_no_repeat_or_gap() {
    use crate::security_check::github_double::recorded_advisories;
    let (first, second) = (recorded_advisories(1), recorded_advisories(2));
    let stopping = Double::repo(RepoRead::Found(named("o", "r")))
        .page(None, first.clone(), Some("c2"))
        .page_read(
            Some("c2"),
            crate::security_check::AdvisoryRead::Failed(RepoFailure {
                code: ErrorCode::RateLimited,
                why: "resets at 2999".into(),
            }),
        );
    let stopped = query_row(
        json_fixture(r#"{"operation": "repository", "owner": "o", "repo": "r"}"#),
        &stopping,
    )
    .await;
    let next = stopped["next"]["nextPage"]["input"]["queries"][0].clone();
    assert!(
        next["resume"].is_string(),
        "a call that reads on: {stopped}"
    );
    let resuming = Double::repo(RepoRead::Found(named("o", "r")))
        .page(None, first.clone(), Some("c2"))
        .page(Some("c2"), second.clone(), None);
    let rest = query_row(next, &resuming).await;
    assert_eq!(
        resuming.asked(),
        [Some("c2".to_owned())],
        "only the unread pages: {rest}"
    );
    assert!(
        rest["next"]["nextPage"].is_null(),
        "the walk is finished: {rest}"
    );
    let mut walked = ids(&stopped);
    walked.extend(ids(&rest));
    let mut published = first
        .iter()
        .chain(&second)
        .map(|advisory| advisory.ghsa_id.clone())
        .collect::<Vec<_>>();
    walked.sort();
    published.sort();
    assert_eq!(walked, published, "every advisory once: {stopped} {rest}");
}

/// A resumed walk refused again at the page it resumes keeps that cursor: the
/// next call asks the same page, never the walk from its start.
#[tokio::test]
async fn a_resumed_advisory_walk_refused_at_its_first_page_keeps_its_cursor() {
    use crate::security_check::github_double::recorded_advisories;
    let refused = || {
        crate::security_check::AdvisoryRead::Failed(RepoFailure {
            code: ErrorCode::RateLimited,
            why: "resets at 2999".into(),
        })
    };
    let stopping = Double::repo(RepoRead::Found(named("o", "r")))
        .page(None, recorded_advisories(1), Some("c2"))
        .page_read(Some("c2"), refused());
    let stopped = query_row(
        json_fixture(r#"{"operation": "repository", "owner": "o", "repo": "r"}"#),
        &stopping,
    )
    .await;
    let next = stopped["next"]["nextPage"]["input"]["queries"][0].clone();
    let refusing = Double::repo(RepoRead::Found(named("o", "r"))).page_read(Some("c2"), refused());
    let again = query_row(next.clone(), &refusing).await;
    assert_eq!(refusing.asked(), [Some("c2".to_owned())], "{again}");
    assert_eq!(
        again["next"]["nextPage"]["input"]["queries"][0]["resume"], next["resume"],
        "the same page is one call away: {again}"
    );
}

/// The repository row in both views: the summary counts the published
/// advisories, names the most severe and links it; `details` lists each with
/// the repository's facts, and is what the summary's one lead returns.
#[tokio::test]
async fn a_repository_summary_counts_and_links_and_details_lists() {
    use crate::security_check::github_double::recorded_advisories;
    let github =
        || Double::repo(RepoRead::Found(named("o", "r"))).page(None, recorded_advisories(1), None);
    let query =
        |view: &str| json!({"operation": "repository", "owner": "o", "repo": "r", "view": view});
    let summary = query_row(query("summary"), &github()).await;
    let details = query_row(query("details"), &github()).await;
    let listed = details["advisories"].as_array().expect("advisories");
    let counted = listed
        .iter()
        .filter(|row| row.get("withdrawn").is_none())
        .collect::<Vec<_>>();
    assert!(!counted.is_empty(), "{details}");
    assert_eq!(summary["advisoryCount"], counted.len(), "{summary}");
    let worst = counted
        .iter()
        .min_by_key(|row| crate::security_check::record::severity_rank(row))
        .expect("a counted advisory");
    assert_eq!(summary["highestSeverity"], worst["severity"], "{summary}");
    assert_eq!(summary["link"], worst["link"], "{summary}");
    for detail in ["repository", "sources"] {
        assert!(details.get(detail).is_some(), "{detail}: {details}");
        assert!(summary.get(detail).is_none(), "{detail}: {summary}");
    }
    // The summary lists each counted advisory once, briefly.
    let brief = summary["advisories"].as_array().expect("brief rows");
    assert_eq!(brief.len(), counted.len(), "{summary}");
    assert!(
        brief.iter().all(|row| row.get("details").is_none()),
        "{summary}"
    );
    // The sentence stays: it says the count is the repository's own
    // published advisories, not the package's.
    assert!(
        summary["answer"]
            .as_str()
            .is_some_and(|answer| answer.contains("published repository advisories")),
        "{summary}"
    );
    assert!(summary.get("related").is_none(), "{summary}");
    octocode_runtime_base::test_support::assert_row_contract(ToolId::SecurityCheck, &summary);
    // Cautions and a source that did not answer stay in the summary.
    let archived = Double::repo(RepoRead::Found(RepoFacts {
        archived: true,
        ..named("o", "r")
    }))
    .page_read(
        None,
        crate::security_check::AdvisoryRead::Failed(RepoFailure {
            code: ErrorCode::RateLimited,
            why: "resets at 2999".into(),
        }),
    );
    let summary = query_row(query("summary"), &archived).await;
    assert_eq!(summary["cautions"][0]["rule"], "archived", "{summary}");
    assert_eq!(summary["isPartial"], true, "{summary}");
    assert_eq!(summary["sources"][0]["source"], "github", "{summary}");
    assert!(summary["sources"][0]["why"].is_string(), "{summary}");
    assert!(
        summary.get("advisoryCount").is_none(),
        "not counted: {summary}"
    );
}

/// GitHub's rate limit is the credential's, the same on every row of a
/// batch: its words (the reset time, the sign-in) are said once for the
/// response, and each row's GitHub entry keeps its code and what it missed.
#[tokio::test]
async fn a_rate_limit_is_said_once_per_response() {
    let cause = "GitHub rate limit reached (rateLimited); it resets at 2999-01-01T00:00:00Z. Run `octocode auth login` for a higher quota.";
    let limited = || {
        RepoRead::Failed(RepoFailure {
            code: ErrorCode::RateLimited,
            why: cause.into(),
        })
    };
    let mut rows = Vec::new();
    for (index, repo) in ["o/r", "o/s"].into_iter().enumerate() {
        let data = row(limited(), repo).await.expect("row");
        rows.push(json!({"index": index, "data": data}));
    }
    let mut response = json!({ "results": rows });
    octocode_runtime_base::response::notices::place(&mut response, "results");
    let text = response.to_string();
    assert_eq!(text.matches("resets at 2999").count(), 1, "{response}");
    // Said for the whole batch, so no row lacks the reset time.
    assert_eq!(response["warnings"], json!([cause]), "{response}");
    for row in response["results"].as_array().expect("rows") {
        let entry = &row["data"]["sources"][0];
        assert_eq!(entry["errorCode"], "rateLimited", "{row}");
        assert_eq!(entry["why"], "Repository not read.", "{row}");
        assert_eq!(row["data"]["isPartial"], true, "{row}");
    }
    // A refusal only one row meets stays that row's own words, on its entry.
    let failure = RepoFailure {
        code: ErrorCode::ProviderError,
        why: "GitHub did not answer (providerError); retry later.".into(),
    };
    let data = row(RepoRead::Failed(failure), "o/r").await.expect("row");
    assert_eq!(
        data["sources"][0]["why"], "GitHub did not answer (providerError); retry later.",
        "{data}"
    );
    assert!(data.get("notices").is_none(), "{data}");
}
