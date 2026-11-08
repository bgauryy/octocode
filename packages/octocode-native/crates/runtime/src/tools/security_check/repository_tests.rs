//! The `repository` row against a fake GitHub at the external boundary.
use super::*;
use crate::providers::artifact::test_http::{RouteHttp, test_budget};
use crate::providers::saved_answers::ReusedSince;
use crate::tools::security_check::github_double::Double;

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
        pushed_at: Some(crate::civil_date::iso8601_millis(pushed)),
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
    let config = crate::tools::security_check::tests::default_config();
    run(
        repo,
        None,
        serde_json::Map::new(),
        &Net {
            http: &http,
            budget: &budget,
            answers: None,
            reused: &reused,
            config: &config,
            github: Some(&fake),
            osv_api: crate::execution::RuntimeLimits::DEFAULT.osv_api,
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
    assert!(data["repository"]["observedAt"].is_string());
    assert_eq!(
        data["policy"]["lowTractionStars"],
        policy.low_traction_stars
    );
    assert_eq!(
        data["policy"]["unmaintainedAfterDays"],
        policy.unmaintained_after_days
    );
    assert_eq!(data["cautions"][0]["rule"], "lowTraction");
    assert_eq!(data["sources"][0]["status"], "found");
}

#[tokio::test]
async fn a_name_that_is_not_a_github_repository_is_refused() {
    for bad in ["justaname", "a/b/c", "/r", "o/"] {
        let error = row(RepoRead::Missing, bad).await.expect_err("refused");
        assert_eq!(error.code, ErrorCode::InvalidInput, "{bad}");
    }
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
    let config = crate::tools::security_check::tests::default_config();
    let failed = FailedRepository("transport broke".into());
    let data = run(
        "o/r",
        None,
        serde_json::Map::new(),
        &Net {
            http: &http,
            budget: &budget,
            answers: None,
            reused: &reused,
            config: &config,
            github: Some(&failed),
            osv_api: crate::execution::RuntimeLimits::DEFAULT.osv_api,
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
async fn query_row(query: Value, github: &Double) -> Value {
    let http = RouteHttp::new();
    let (budget, reused) = (test_budget(), ReusedSince::default());
    let config = crate::tools::security_check::tests::default_config();
    let query: octocode_contract_types::SecurityCheckQuery =
        serde_json::from_value(query).expect("query");
    crate::tools::security_check::run(
        &query,
        &crate::tools::security_check::github_double::net(&http, &budget, &reused, &config, github),
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
    use crate::tools::security_check::github_double::recorded_advisories;
    let (first, second) = (recorded_advisories(1), recorded_advisories(2));
    let stopping = Double::repo(RepoRead::Found(named("o", "r")))
        .page(None, first.clone(), Some("c2"))
        .page_read(
            Some("c2"),
            crate::tools::security_check::AdvisoryRead::Failed(RepoFailure {
                code: ErrorCode::RateLimited,
                why: "resets at 2999".into(),
            }),
        );
    let stopped = query_row(json!({"operation": "repository", "repo": "o/r"}), &stopping).await;
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
