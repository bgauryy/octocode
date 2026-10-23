//! Whether clasify is offered: beta and key gating, and inputs rejected at validation before any work.

use super::{noul, provider, state_row};
use crate::support::{Run as _, Workspace, clasify_runtime, provider_env};
use octocode_contract_types::ErrorCode;
use serde_json::json;

#[tokio::test]
async fn retired_jev_vendor_key_does_not_enable_clasify() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_JEV_KEY", "jev-native-secret".into())]);
    assert!(!runtime.is_available("clasify"));
    runtime.close().await;
}

#[tokio::test]
async fn public_clasify_requires_beta_and_a_nonblank_key_independently() {
    let workspace = Workspace::new();
    for (env, missing) in [
        (
            vec![("OCTOCODE_CLASSIFICATION_API", "secret".into())],
            "OCTOCODE_BETA",
        ),
        (
            vec![("OCTOCODE_BETA", "true".into())],
            "OCTOCODE_CLASSIFICATION_API",
        ),
        (
            vec![
                ("OCTOCODE_BETA", "true".into()),
                ("OCTOCODE_CLASSIFICATION_API", "  ".into()),
            ],
            "OCTOCODE_CLASSIFICATION_API",
        ),
    ] {
        let runtime = workspace.runtime(&env);
        assert!(!runtime.is_available("clasify"), "{env:?}");
        let error = runtime
            .run(
                "missing-gate".into(),
                "clasify".into(),
                json!({"queries":[state_row()]}),
            )
            .await
            .expect_err("an unmet prerequisite blocks public clasify");
        assert_eq!(error.code, ErrorCode::MissingConfiguration);
        assert!(error.message.contains(missing), "{}", error.message);
        runtime.close().await;
    }
}

#[tokio::test]
async fn configured_clasify_is_available_without_contacting_the_provider() {
    let server = provider(0).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    assert!(runtime.is_available("clasify"));
    runtime.close().await;
}

#[tokio::test]
async fn a_disabled_reader_is_rejected_at_validation_before_any_work() {
    let server = provider(0).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&provider_env(
        &server,
        &[("DISABLE_TOOLS", "localFetch".into())],
    ));
    let error = runtime
        .run(
            "disabled-reader".into(),
            "clasify".into(),
            json!({"queries":[{"source":{"tool":"localFetch","query":{"path":"a.rs","ranges":["1-2"]}},
                "questions":{"q":noul("Anything?")}}]}),
        )
        .await
        .expect_err("rejected at validation");
    assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
    let issues = error.validation_issues.expect("field issues");
    assert_eq!(issues[0].rule_id, "clasify.source-reader-available");
    assert_eq!(issues[0].path, ["queries", "0", "source"]);
    assert!(
        issues[0].message.contains("localFetch"),
        "{}",
        issues[0].message
    );
    runtime.close().await;
}

#[tokio::test]
async fn removed_matrix_input_is_rejected_not_converted() {
    let server = provider(0).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    for legacy in [
        json!({"resources":[{"value":"x"}],"questions":[{"type":"yesno","ask":"Relevant?"}]}),
        json!({"state":"x","questions":{"q":{"type":"locate","instructions":"Where?"}}}),
        json!({"state":"x","questions":{"q":noul("x")},"resources":[{"value":"x"}]}),
    ] {
        let error = runtime
            .run(
                "legacy".into(),
                "clasify".into(),
                json!({"queries":[legacy.clone()]}),
            )
            .await
            .expect_err("legacy shape");
        assert_eq!(error.code, ErrorCode::InvalidInput, "{legacy}");
    }
    runtime.close().await;
}
