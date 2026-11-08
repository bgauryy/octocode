//! Rows of the clasify audit (CLS-C21, CLS-C25) reproduced against a stub provider: no billed call.

use super::{clasify, content_row, provider};
use crate::support::{Workspace, clasify_runtime};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// CLS-C21: a row whose provider call failed is a failed row carrying the
/// provider's error code, so a batch of only failures ends as a failure
/// (`ToolOutcome::all_failed`) and is never an answer-shaped success.
#[tokio::test]
async fn a_batch_whose_every_row_failed_is_failed_not_answered() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(&runtime, vec![content_row(), content_row()]).await;
    let rows = out["results"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "{out}");
    for row in rows {
        assert_eq!(row["status"], "error", "{out}");
        assert_eq!(
            row["data"]["errorCode"], "classificationProviderError",
            "{out}"
        );
        assert!(row["data"].get("answers").is_none(), "{out}");
    }
    runtime.close().await;
}

/// CLS-C25 (the audit's claim, that usage repeats on identical rows, is
/// wrong): identical rows share one provider exchange, the first row states
/// its usage and the others state none, so a sum over rows counts the one
/// billed exchange once.
#[tokio::test]
async fn identical_rows_share_one_exchange_and_its_usage_is_counted_once() {
    let server = provider(1).await;
    let workspace = Workspace::new();
    let runtime = clasify_runtime(&workspace, &server, &[]);
    let out = clasify(&runtime, vec![content_row(), content_row()]).await;
    assert_eq!(super::sent(&server).await.len(), 1, "{out}");
    let calls = |index: usize| out["results"][index]["data"]["usage"]["calls"].clone();
    assert_eq!(calls(0), 1, "{out}");
    assert_eq!(calls(1), 0, "{out}");
    let tokens = |index: usize| out["results"][index]["data"]["usage"]["input_tokens"].clone();
    assert_eq!(tokens(0), 40, "{out}");
    assert_eq!(tokens(1), 0, "{out}");
    runtime.close().await;
}
