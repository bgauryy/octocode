//! Package trust signals against a recorded registry and a fake GitHub.
use super::*;
use crate::providers::artifact::test_http::{RouteHttp, test_budget};
use crate::providers::saved_answers::ReusedSince;
use crate::tools::security_check::github_double::Double;
use crate::tools::security_check::repository::{RepoFacts, RepoFailure};
use crate::tools::security_check::{Net, RepositoryRead};
use octocode_contract_types::ErrorCode;

const DAY_MILLIS: i64 = 86_400_000;

fn contract() -> &'static SecurityContract {
    security_contract().expect("security block")
}

fn facts(stars: u64, days_since_push: u64) -> RepoFacts {
    let pushed = crate::civil_date::unix_millis()
        - i64::try_from(days_since_push).expect("days") * DAY_MILLIS;
    RepoFacts {
        owner: "o".into(),
        name: "r".into(),
        stars: usize::try_from(stars).expect("stars"),
        pushed_at: Some(crate::civil_date::iso8601_millis(pushed)),
        ..RepoFacts::default()
    }
}

fn registered(url: Option<&str>) -> Registered {
    Registered {
        repository: url.map(str::to_owned),
        ..Registered::default()
    }
}

async fn row(registered: Registered, github: Option<&dyn RepositoryRead>) -> Value {
    let http = RouteHttp::new();
    let budget = test_budget();
    let reused = ReusedSince::default();
    let net = Net {
        http: &http,
        budget: &budget,
        answers: None,
        reused: &reused,
        config: &crate::tools::security_check::tests::default_config(),
        github,
        osv_api: crate::execution::RuntimeLimits::DEFAULT.osv_api,
    };
    let mut data = json!({"sources": [{"source": "osv", "status": "found"}]});
    let confirmed = crate::tools::security_check::source::Confirmed {
        ecosystem: crate::providers::artifact::query::artifact_type::osv_ecosystem(
            octocode_contract_types::ArtifactType::Npm,
        ),
        name: "lodash".into(),
        version: Some("4.17.20".into()),
        resume: None,
        release: None,
        import: None,
    };
    read(registered, &confirmed, &net).await.apply(&mut data);
    data
}

fn rules(data: &Value) -> Vec<&str> {
    data["cautions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|caution| caution["rule"].as_str())
        .collect()
}

#[tokio::test]
async fn stars_and_days_fire_one_past_the_contract_thresholds() {
    let policy = &contract().policy;
    let url = Some("https://github.com/o/r");
    let at = Double::repo(RepoRead::Found(facts(
        policy.low_traction_stars,
        policy.unmaintained_after_days,
    )));
    let data = row(registered(url), Some(&at)).await;
    assert!(rules(&data).is_empty(), "{data}");
    let low = Double::repo(RepoRead::Found(facts(
        policy.low_traction_stars - 1,
        policy.unmaintained_after_days,
    )));
    assert_eq!(
        rules(&row(registered(url), Some(&low)).await),
        ["lowTraction"]
    );
    let old = Double::repo(RepoRead::Found(facts(
        policy.low_traction_stars,
        policy.unmaintained_after_days + 1,
    )));
    assert_eq!(
        rules(&row(registered(url), Some(&old)).await),
        ["unmaintained"]
    );
}

#[tokio::test]
async fn a_found_repository_fills_the_block_policy_and_source() {
    let github = Double::repo(RepoRead::Found(facts(5000, 1)));
    let data = row(
        registered(Some("git+https://github.com/o/r.git")),
        Some(&github),
    )
    .await;
    assert_eq!(data["repository"]["stars"], 5000, "{data}");
    assert_eq!(data["repository"]["provenance"], "claimed");
    assert_eq!(
        data["policy"]["lowTractionStars"],
        contract().policy.low_traction_stars
    );
    assert_eq!(data["sources"][1]["source"], "github");
    assert_eq!(data["sources"][1]["status"], "found");
    assert!(data.get("cautions").is_none(), "{data}");
}

#[tokio::test]
async fn registry_and_github_rules_fire_with_the_contract_levels() {
    let mut named = registered(Some("https://github.com/o/r"));
    named.deprecated = Some("use p2".into());
    named.yanked = true;
    let mut archived = facts(5000, 1);
    archived.archived = true;
    let github = Double::repo(RepoRead::Found(archived));
    let data = row(named.clone(), Some(&github)).await;
    assert_eq!(rules(&data), ["deprecated", "yanked", "archived"]);
    let missing = Double::repo(RepoRead::Missing);
    let data = row(named, Some(&missing)).await;
    assert!(rules(&data).contains(&"repositoryMissing"), "{data}");
    for caution in data["cautions"].as_array().expect("cautions") {
        let rule = contract()
            .rule(caution["rule"].as_str().expect("rule"))
            .expect("rule");
        let level = match rule.level {
            crate::contracts::security::CautionLevel::Review => "review",
            crate::contracts::security::CautionLevel::Note => "note",
        };
        assert_eq!(caution["level"], level, "{caution}");
    }
}

#[tokio::test]
async fn a_failed_trust_read_is_disclosed_and_keeps_the_osv_source() {
    let reason = "GitHub rate limit spent; resets at 2999-01-01T00:00:00Z.";
    let github = Double::repo(RepoRead::Failed(RepoFailure {
        code: ErrorCode::RateLimited,
        why: reason.into(),
    }));
    let data = row(registered(Some("https://github.com/o/r")), Some(&github)).await;
    assert_eq!(data["sources"][0]["source"], "osv");
    assert_eq!(data["sources"][1]["status"], "failed");
    assert_eq!(data["sources"][1]["why"], reason);
    assert_eq!(data["isPartial"], true, "{data}");
    assert!(data["partialReasons"].to_string().contains("sourceFailed"));
    assert!(
        data.get("repository").is_none() && data.get("policy").is_none(),
        "{data}"
    );
}

#[tokio::test]
async fn no_repository_is_a_note_and_a_foreign_host_is_not_covered() {
    let data = row(registered(None), None).await;
    assert_eq!(rules(&data), ["noSourceRepo"]);
    assert_eq!(data["cautions"][0]["level"], "note");
    assert_eq!(data["sources"].as_array().expect("sources").len(), 1);
    let data = row(registered(Some("https://gitlab.com/o/r")), None).await;
    assert_eq!(data["sources"][1]["status"], "notCovered", "{data}");
    assert!(data.get("cautions").is_none(), "{data}");
}

/// A review caution is named in the answer, so "none known to OSV" never stands
/// alone beside it; a note-level caution is not, and the raw text stays whole.
#[test]
fn the_answer_names_review_cautions_only() {
    let message = "use something else ".repeat(50);
    let trust = Trust {
        registered: Registered {
            deprecated: Some(message.clone()),
            ..registered(None)
        },
        repo: Repo::Unnamed,
        advisories: None,
    };
    let mut data = json!({"answer": "x@1: none known to OSV."});
    trust.apply(&mut data);
    let answer = data["answer"].as_str().expect("answer");
    assert!(answer.starts_with("x@1: none known to OSV."), "{answer}");
    assert!(answer.contains("deprecated"), "{answer}");
    assert!(!answer.contains("noSourceRepo"), "{answer}");
    assert!(
        data["cautions"][0]["why"]
            .as_str()
            .expect("why")
            .contains(message.trim()),
        "the registry text is kept whole"
    );
    let mut note_only = json!({"answer": "x@1: none known to OSV."});
    Trust {
        registered: registered(None),
        repo: Repo::Unnamed,
        advisories: None,
    }
    .apply(&mut note_only);
    assert_eq!(note_only["answer"], "x@1: none known to OSV.");
}

/// A registry's deprecation message is registry text: the output gate redacts a
/// secret in it as in any other leaf, and a long one reaches the row whole.
#[tokio::test]
async fn a_deprecation_message_is_redacted_by_the_gate_and_never_cut() {
    let secret = format!("ghp_{}", "a1B2c3D4e5".repeat(4));
    let (head, tail) = ("HEAD-OF-MESSAGE", "TAIL-OF-MESSAGE");
    let filler = "use the maintained fork instead. ".repeat(400);
    let message = format!("{head} {filler}token {secret} {filler}{tail}");
    let data = row(
        Registered {
            deprecated: Some(message.clone()),
            ..registered(Some("https://example.com/o/r"))
        },
        None,
    )
    .await;
    let shown = crate::security::Sanitized::sealed(data);
    let why = shown["cautions"]
        .as_array()
        .expect("cautions")
        .iter()
        .find(|caution| caution["rule"] == "deprecated")
        .expect("a deprecated caution")["why"]
        .as_str()
        .expect("why")
        .to_owned();
    assert!(!why.contains(&secret), "{why}");
    assert!(why.contains("REDACTED"), "{why}");
    assert!(
        why.contains(head) && why.ends_with(tail),
        "cut: {}",
        why.len()
    );
    assert_eq!(
        why.matches(filler.as_str()).count(),
        2,
        "cut inside the text"
    );
}
