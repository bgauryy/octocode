use super::*;

#[test]
fn missing_content_is_not_an_empty_file() {
    for encoding in ["base64", "utf-8"] {
        let result = decode_content(Some(encoding), None);
        assert!(
            matches!(result, Err(error) if error.kind == ProviderErrorKind::Decode),
            "missing {encoding} content must be a decode failure"
        );
    }
}

#[test]
fn explicitly_empty_content_is_an_empty_file() {
    for encoding in ["base64", "utf-8"] {
        assert_eq!(
            decode_content(Some(encoding), Some(String::new())).expect("empty file"),
            Vec::<u8>::new()
        );
    }
}

#[test]
fn a_submodule_marker_takes_precedence_over_the_file_type() {
    for kind in ["file", "submodule"] {
        let body = serde_json::to_vec(&serde_json::json!({
            "type": kind,
            "submodule_git_url": "https://github.com/acme/dependency.git",
            "sha": "0123456789abcdef0123456789abcdef01234567"
        }))
        .expect("payload");
        let result = parse_content_payload(&body, "vendor/dependency");
        assert!(
            matches!(result, Err(error) if error.reason == Some(crate::ProviderErrorReason::Submodule)),
            "{kind} entry with a submodule URL must not be read as a file"
        );
    }
}

#[test]
fn a_null_submodule_url_does_not_reclassify_a_regular_file() {
    let body = br#"{"type":"file","submodule_git_url":null,"encoding":"base64","content":""}"#;
    assert!(parse_content_payload(body, "empty.rs").is_ok());
}
