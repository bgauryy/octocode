use super::*;
use octocode_contract_types::ErrorCode;

/// A sections row fails only when no resource was read: then as its
/// reads did, one code, else a failed context.
#[test]
fn a_sections_row_with_no_resource_read_fails_like_its_reads() {
    let resource =
        |status: &str, code: &str| json!({"status":status,"error":{"errorCode":code,"error":"x"}});
    let row = |resources: Value| json!({"operation":"sections","resources":resources});
    let missing = resource("error", "pathNotFound");
    assert_eq!(
        unread_resources_code(&row(json!({"a":missing,"b":missing}))),
        Some(ErrorCode::PathNotFound)
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
    let listing = json!({"models":[
        {"name":"jev-1.13","description":"d","release_date":"2026-01-01"},
        {"name":"jev-1.14","description":"d","release_date":"2026-06-01"}
    ]});
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
