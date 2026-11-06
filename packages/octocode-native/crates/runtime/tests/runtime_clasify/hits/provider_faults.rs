//! What a slow or refusing provider leaves of a judged view: the hits stay readable,
//! and the real cause is named.
use super::*;

/// A provider slower than the network layer's request time still answers, on
/// every route: no client cap bounds a classification, only the call's own
/// deadline does (a 40 s provider reply once failed at the 30 s network time).
#[tokio::test]
async fn a_slow_provider_is_waited_for_on_every_route() {
    let limits = octocode_native::execution::RuntimeLimits {
        network_timeout: std::time::Duration::from_secs(1),
        classification_timeout: None,
        ..support::FIXTURE_LIMITS
    };
    let delay = limits.network_timeout * 2;
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| answer(request).set_delay(delay))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("dense.txt", DENSE);
    let runtime = workspace.runtime_with(&support::provider_env(&server, &[]), limits);
    let row = support::run(
        &runtime,
        "localSearch",
        json!({"path":file,"matchString":["needle"],"contextLines":{"before":0,"after":0},
            "classification":{"predicates":[PREDICATE]}}),
    )
    .await;
    assert_eq!(
        row.data()["selection"]["selectedItems"],
        3,
        "{}",
        row.data()
    );
    let sections = support::run(
        &runtime,
        "clasify",
        json!({"operation":"sections","resources":{"a":{"tool":"localFetch","query":{"path":file}}},
            "questions":{"q":{"type":"score","instructions":"Says KEEP?","criteria":["no","yes"]}}}),
    )
    .await;
    let coverage = &sections.data()["resources"]["a"]["coverage"];
    assert!(
        coverage["answered"].as_u64().unwrap() > 0,
        "{}",
        sections.data()
    );
    assert!(coverage.get("error").is_none(), "{}", sections.data());
    runtime.close().await;
}

/// A provider past the request time never hides hits: they come back as
/// found, the cause named, and a later retry is offered as a plain call.
#[tokio::test]
async fn a_timeout_shows_the_hits_and_names_the_real_cause() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &Request| {
            answer(request).set_delay(std::time::Duration::from_secs(3))
        })
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let file = workspace.write("dense.txt", DENSE);
    let limits = octocode_native::execution::RuntimeLimits {
        classification_timeout: Some(std::time::Duration::from_millis(200)),
        ..support::PROVIDER_LIMITS
    };
    let runtime = workspace.runtime_with(&support::provider_env(&server, &[]), limits);
    let row = support::run(
        &runtime,
        "localSearch",
        json!({"path":file,"matchString":["needle"],"contextLines":{"before":0,"after":0},
            "classification":{"predicates":[PREDICATE]}}),
    )
    .await;
    let data = row.data();
    assert_ne!(row.status(), "error", "{data}");
    assert_eq!(
        kept_hit_lines(data).len(),
        6,
        "every hit stays visible: {data}"
    );
    assert_eq!(data["selection"]["status"], "unavailable", "{data}");
    assert_eq!(data["selection"]["reason"], "timeout", "{data}");
    let warnings = data["warnings"].to_string();
    assert!(warnings.contains("slow"), "the real cause: {data}");
    assert!(
        !warnings.contains("smaller request"),
        "the wrong hint: {data}"
    );
    runtime.close().await;
}

/// A `choice`-shaped `criteria` on a `score` is refused, showing the array
/// form; it never becomes a one-level scale with confidence 1.
#[tokio::test]
async fn a_score_with_choice_shaped_criteria_is_refused_with_the_correct_form() {
    let server = provider().await;
    let workspace = Workspace::new();
    let runtime = support::clasify_runtime(&workspace, &server, &[]);
    for criteria in [
        json!({"0":"low","1":"high"}),
        json!("low"),
        json!(["only one"]),
    ] {
        let refused = support::call(
            &runtime,
            "clasify",
            json!({"operation":"evaluate","content":"text",
                "questions":{"q":{"type":"score","instructions":"How high?","criteria":criteria}}}),
        )
        .await
        .err()
        .unwrap_or_else(|| panic!("{criteria}: not refused"));
        let shown = format!("{:?}", refused.payload);
        assert!(shown.contains("invalidInput"), "{criteria}: {shown}");
        assert!(shown.contains("criteria"), "{criteria}: {shown}");
        assert!(
            shown.contains("at least two levels"),
            "shows the correct form: {criteria}: {shown}"
        );
    }
    assert!(crate::runtime_clasify::sent(&server).await.is_empty());
    runtime.close().await;
}
