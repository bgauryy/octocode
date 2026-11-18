//! Where the response stage states a static notice: once, on the first row
//! that carries it, or once for the call when rows with evidence share it.
use super::place;
use serde_json::{Value, json};

const CAUSE: &str = "GitHub rate limit reached (rateLimited); it resets at 2999-01-01T00:00:00Z. Run `octocode auth login` for a higher quota.";

fn evidence(notices: &[&str]) -> Value {
    json!({"data": {"files": [], "notices": notices}})
}

fn failed(notices: &[&str]) -> Value {
    json!({"status": "error", "data": {"errorCode": "rateLimited", "error": "Limited.", "notices": notices}})
}

/// A notice that two or more rows with evidence carry is a warning those
/// rows share: the envelope states it once, in its `warnings`, so every row
/// of the batch has the cause and reset time, and no row repeats it. Each
/// row keeps the notices that are its own.
#[test]
fn a_notice_rows_with_evidence_share_is_stated_once_for_the_call() {
    let mut output =
        json!({"results": [evidence(&[CAUSE]), evidence(&[]), evidence(&[CAUSE, "Own."])]});
    place(&mut output, "results");
    assert_eq!(output["warnings"], json!([CAUSE]), "{output}");
    assert!(
        output["results"][0]["data"].get("warnings").is_none(),
        "{output}"
    );
    assert_eq!(
        output["results"][2]["data"]["warnings"],
        json!(["Own."]),
        "{output}"
    );
    assert!(!output.to_string().contains("\"notices\""), "{output}");
}

/// A notice one row carries stays that row's own; on failed rows a notice
/// is guidance, stated in the hints of the first failed row that needs it.
#[test]
fn a_notice_one_row_carries_or_failed_rows_need_stays_on_a_row() {
    let mut alone = json!({"results": [evidence(&[]), evidence(&[CAUSE])]});
    place(&mut alone, "results");
    assert!(alone.get("warnings").is_none(), "{alone}");
    assert_eq!(
        alone["results"][1]["data"]["warnings"],
        json!([CAUSE]),
        "{alone}"
    );
    let mut failures = json!({"results": [failed(&[CAUSE]), failed(&[CAUSE])]});
    place(&mut failures, "results");
    assert!(failures.get("warnings").is_none(), "{failures}");
    assert_eq!(
        failures["results"][0]["data"]["hints"],
        json!([CAUSE]),
        "{failures}"
    );
    assert!(
        failures["results"][1]["data"].get("hints").is_none(),
        "{failures}"
    );
}
