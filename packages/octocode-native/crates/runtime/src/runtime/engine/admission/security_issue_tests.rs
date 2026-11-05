use super::*;

/// A row's own text names no file and leaves for the provider: whatever a read
/// of some file would redact is redacted, in every string of it. A bare shape
/// still needs its context, and plain state is untouched.
#[test]
fn clasify_text_is_judged_as_if_it_could_be_any_file() {
    const VALUE: &str = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIz";
    let manifest = format!("apiVersion: v1\nkind: Secret\ndata:\n  tls.crt: {VALUE}\n");
    let security = crate::security::ContentSecurity::new();
    let policy = InputPolicy::ClasifyEvidence {
        content_limit: 32_000,
    };
    for row in [
        json!({"content": manifest}),
        json!({"content": {"file": {"text": manifest}}}),
        json!({"content": [manifest]}),
        json!({"content": "plain", "questions": {"q": {"type": "noul", "instructions": manifest}}}),
    ] {
        let state = security
            .check_input(row.clone(), policy)
            .expect("redacted in place");
        let shown = state.to_string();
        assert!(!shown.contains(VALUE), "{shown}");
        assert!(shown.contains("REDACTED-KUBERNETESSECRETS"), "{shown}");
    }
    // A `context` is no evidence: refused as any other credential is.
    let issues = security
        .check_input(json!({"content": "plain", "context": manifest}), policy)
        .expect_err("refused");
    assert_eq!(issues[0].path, ["context"], "{issues:?}");
    // A read of no file would redact these, whatever it is called.
    for plain in [
        "kind: ConfigMap\ndata:\n  blob: QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIz\n".to_owned(),
        format!("commit {}\n", "a".repeat(40)),
        "ordinary evidence".to_owned(),
    ] {
        let state = security
            .check_input(json!({"content": plain}), policy)
            .expect("clean");
        assert_eq!(state["content"], json!(plain));
    }
    // The address of a read stays refused, not rewritten.
    let issues = security
        .check_input(
            json!({"source": {"tool": "localFetch", "query": {"matchString": manifest}}}),
            policy,
        )
        .map(|_| Vec::new())
        .unwrap_or_else(|issues| issues);
    assert!(
        issues.is_empty() || issues[0].rule_id == "security.credential",
        "{issues:?}"
    );
}

/// Clasify evidence may be redacted in place; a credential-looking value
/// in a resource read's `query` would run redacted, so it is rejected.
#[test]
fn a_secret_in_a_clasify_source_is_rejected_and_in_state_redacted() {
    let token = format!("ghp_{}", "a".repeat(36));
    let security = crate::security::ContentSecurity::new();
    let policy = InputPolicy::ClasifyEvidence {
        content_limit: 32_000,
    };
    let state = security
        .check_input(json!({"content":{"held":token}}), policy)
        .expect("held state is redacted in place");
    assert!(!state.to_string().contains(&token), "{state:?}");
    let issues = security
        .check_input(
            json!({"source":{"tool":"localFetch","input":{"path":token}}}),
            policy,
        )
        .expect_err("a source locator cannot be silently redacted");
    let paths = issues
        .iter()
        .map(|issue| issue.path.join("."))
        .collect::<Vec<_>>();
    assert_eq!(paths, ["source.input.path"], "{issues:?}");
    assert!(
        issues
            .iter()
            .all(|issue| issue.rule_id == "security.credential"),
        "{issues:?}"
    );
    let issues = security
            .check_input(
                json!({"assets":{"image":{"type":"image","source":{"url":format!("https://example.com/{token}.png")}}}}),
                policy,
            )
            .expect_err("an asset locator cannot be silently redacted");
    assert_eq!(
        issues.len(),
        1,
        "an asset locator cannot be silently redacted"
    );
    assert_eq!(issues[0].path, ["assets.image.source.url"]);
}
