//! The reporter's corpus (octocodecode `cc1bd7e046484b46e9afda186c414edf7d635600`,
//! `packages/mcp/src/oauth/`, copied verbatim under `corpus/`) through
//! localSearch and the real evaluator against a mocked provider: what each
//! request's state holds for the hits it asks about.
use super::*;
use octocode_tools_shared::judgment::Evaluator;
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

const SIGN_IN: &str = include_str!("corpus/sign-in.ts.txt");
const AUTH_PROVIDER: &str = include_str!("corpus/auth-provider.ts.txt");

/// A provider answering every question with direct evidence.
fn direct_answers(request: &Request) -> ResponseTemplate {
    let body: Value = request.body_json().expect("request");
    let answers = body["questions"]
        .as_object()
        .expect("questions")
        .iter()
        .map(|(id, question)| {
            let legend = question["criteria"]
                .as_array()
                .expect("criteria")
                .iter()
                .enumerate()
                .map(|(i, criterion)| (i.to_string(), criterion.clone()))
                .collect::<serde_json::Map<String, Value>>();
            (
                id.clone(),
                json!({"type":"score","score":2.0,"confidence":1.0,
                "legend":legend,"probabilities":{"0":0.0,"1":0.0,"2":1.0}}),
            )
        })
        .collect::<serde_json::Map<String, Value>>();
    ResponseTemplate::new(200).set_body_json(json!({
        "model":"jev-resolved","answers":answers,
        "usage":{"input_tokens":10,"output_tokens":2}
    }))
}

/// The bodies the provider received for a classified localSearch over the
/// corpus, with `fields` added to the query.
fn judged(fields: Value) -> (LocalSearchResult, Vec<Value>) {
    judged_over(
        &[
            ("oauth/sign-in.ts", SIGN_IN),
            ("oauth/auth-provider.ts", AUTH_PROVIDER),
        ],
        direct_answers,
        fields,
    )
}

/// The bodies the provider received, answering with `answers`, for a
/// classified localSearch over `files` (relative path, text).
pub(super) fn judged_over(
    files: &[(&str, &str)],
    answers: fn(&Request) -> ResponseTemplate,
    fields: Value,
) -> (LocalSearchResult, Vec<Value>) {
    let root = tempfile::tempdir().expect("fixture");
    for (path, text) in files {
        let path = root.path().join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(path, text).expect("file");
    }
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let server = executor.block_on(MockServer::start());
    executor.block_on(
        Mock::given(method("POST"))
            .respond_with(answers)
            .mount(&server),
    );
    let evaluator = Evaluator::new(
        Some(secrecy::SecretString::from("test-key".to_owned())),
        server.uri(),
        "jev-test".into(),
        &octocode_runtime_base::providers::classification::jev::JEV,
        Some(Duration::from_secs(10)),
        0,
        4,
    );
    let selection = octocode_tools_shared::test_support::selection_fake::runtime(
        evaluator,
        executor.handle().clone(),
        Duration::from_secs(20),
    );
    let mut query = json!({"path": root.path(), "matchString": [".catch("],
        "classification": {"predicates": ["Does this discard the error without handling it?"], "maxRequests": 1000}});
    for (key, value) in fields.as_object().into_iter().flatten() {
        query[key] = value.clone();
    }
    let query: LocalSearchQuery = serde_json::from_value(query).expect("query");
    let result = execute_local_search(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        &query,
        &octocode_runtime_base::test_support::workspace_policy(root.path()),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        Some(&selection),
    )
    .expect("search");
    let bodies = executor
        .block_on(server.received_requests())
        .expect("requests")
        .iter()
        .map(|request| request.body_json().expect("body"))
        .collect();
    (result, bodies)
}

/// Each question's target label, with the numbered window lines that follow
/// it, keyed by the line the label names.
fn targets(body: &Value) -> Vec<(u64, String)> {
    body["questions"]
        .as_object()
        .expect("questions")
        .values()
        .filter_map(|question| {
            let label = question["instructions"]["target"].as_str()?;
            let line = label.strip_prefix("line ")?.split(':').next()?.parse().ok()?;
            Some((line, label.to_owned()))
        })
        .collect()
}

/// Every hit is judged over its own line alone: its question carries that
/// line (`sign-in.ts:88` and `:89` have identical code), no pooled state holds
/// other hits' lines, and each of the five `.catch(` hits is kept by a judge
/// that finds direct evidence.
#[test]
fn every_corpus_hit_is_judged_over_its_own_line() {
    for after in [0, 1, 3] {
        let (result, bodies) = judged(json!({"contextLines":{"before":0,"after":after}}));
        assert_eq!(bodies.len(), 2, "one request per file");
        let mut asked = Vec::new();
        for body in &bodies {
            assert!(
                body["state"]["lines"].as_str().unwrap_or_default().is_empty(),
                "after {after}: hits share no pooled lines"
            );
            for (target, label) in targets(body) {
                let own = format!("\n{target}\t");
                assert!(
                    label.lines().next().is_some_and(|head| head.contains(".catch("))
                        || label.contains(&own),
                    "after {after}: {} line {target} missing from its question:\n{label}",
                    body["state"]["path"]
                );
                asked.push((body["state"]["path"].to_string(), target));
            }
        }
        asked.sort();
        assert_eq!(asked.len(), 5, "after {after}: {asked:?}");
        let receipt = result.selection.as_ref().expect("receipt");
        assert_eq!(receipt.selected_items, 5, "after {after}");
    }
}
