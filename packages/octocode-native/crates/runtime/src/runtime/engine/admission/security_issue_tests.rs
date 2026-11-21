use super::*;
use octocode_test_support::json_fixture;

/// A row's own text names no file and leaves for the provider. Its
/// evidence (`content`) is judged as a read judges text of no known file, so
/// the provider reads what localFetch shows of it; the caller's words beside
/// it lose whatever a read of some file would redact, refusing nothing: no
/// rule knows it for a credential the caller wrote. A bare shape still needs
/// its context, and plain state is untouched.
#[test]
fn clasify_text_is_judged_as_if_it_could_be_any_file() {
    const VALUE: &str = "ghp_QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIz";
    let manifest = format!("apiVersion: v1\nkind: Config\ndata:\n  token: {VALUE}\n");
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let policy = InputPolicy::ClasifyEvidence {
        content_limit: 32_000,
    };
    let read = security.sanitize_text(&manifest, None).content.into_owned();
    for (row, evidence) in [
        (json!({"content": manifest}), json!(read)),
        (
            json!({"content": {"file": {"text": manifest}}}),
            json!({"file": {"text": read}}),
        ),
        (json!({"content": [manifest]}), json!([read])),
    ] {
        let state = security
            .check_input(row.clone(), policy)
            .expect("redacted in place");
        assert_eq!(state["content"], evidence, "{row}");
    }
    // The caller's words holding a token hold what a read of them hides, so
    // the screen refuses them with the prose repair.
    for (row, field) in [
        (
            json!({"content": "plain", "questions": {"q": {"type": "noul", "instructions": manifest}}}),
            "questions.q.instructions",
        ),
        (
            json!({"content": "plain", "background": manifest}),
            "background",
        ),
    ] {
        let issues = security
            .check_input(row.clone(), policy)
            .map(|_| Vec::new())
            .unwrap_or_else(|issues| issues);
        assert!(
            issues.iter().any(|issue| issue.path == [field]
                && issue
                    .message
                    .contains("remove the credential from the text")),
            "{row}: {issues:?}"
        );
    }
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

/// One rule for a credential the caller wrote, whatever field holds it: only
/// `content`, the evidence under judgment, is redacted in place. A question,
/// its criteria and the task's context are the caller's words, refused as a
/// search term or a `classification.predicates` entry is, and the ordinary
/// look-alike runs in each.
#[test]
fn a_credential_in_the_callers_words_is_refused_in_every_clasify_field() {
    let token = format!("ghp_{}", "a1B2".repeat(9));
    let password = "aws_secret_access_key = Tr0ub4dorXK9zQwTr0ub4dorXK9zQwTr0ub4dorX";
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let policy = InputPolicy::ClasifyEvidence {
        content_limit: 32_000,
    };
    let row = |field: &str, text: &str| match field {
        "instructions" => json!({"content": "plain",
            "questions": {"q": {"type": "noul", "instructions": text}}}),
        "criteria" => json!({"content": "plain", "questions": {"q": {"type": "noul",
            "instructions": "Is this a greeting?", "criteria": {"true": text, "false": "no"}}}}),
        "background" => json!({"content": "plain", "background": text,
            "questions": {"q": {"type": "noul", "instructions": "Is this a greeting?"}}}),
        _ => json!({"content": text,
            "questions": {"q": {"type": "noul", "instructions": "Is this a greeting?"}}}),
    };
    for secret in [
        format!("Does the text contain {token}?"),
        format!("Does the text contain {password}?"),
    ] {
        for (field, path) in [
            ("instructions", "questions.q.instructions"),
            ("criteria", "questions.q.criteria.true"),
            ("background", "background"),
        ] {
            let issues = security
                .check_input(row(field, &secret), policy)
                .expect_err("the caller's words are refused");
            assert_eq!(issues.len(), 1, "{field}: {issues:?}");
            assert_eq!(issues[0].rule_id, "security.credential", "{field}");
            assert_eq!(issues[0].path, [path], "{field}");
        }
        let state = security
            .check_input(row("content", &secret), policy)
            .expect("evidence is redacted in place");
        let shown = state.to_string();
        assert!(
            !shown.contains(&token) && !shown.contains("Tr0ub4dorXK9"),
            "{shown}"
        );
        assert!(shown.contains("[REDACTED"), "{shown}");
    }
    for ordinary in [
        "Does the function take a password: str parameter?",
        "Is token: Token declared, or api_key: str = None?",
    ] {
        for field in ["instructions", "criteria", "background"] {
            let state = security
                .check_input(row(field, ordinary), policy)
                .unwrap_or_else(|issues| panic!("{field} {ordinary:?}: {issues:?}"));
            assert!(
                state.to_string().contains(ordinary),
                "{field} {ordinary:?}: {state:?}"
            );
        }
    }
}

/// A key's armor line alone and a URL with no password are no credentials,
/// so they refuse no field, and the question that passed the gate is the
/// question asked: the provider reads the caller's words as a GitHub search
/// sends them, not a placeholder nobody asked about.
#[test]
fn what_the_gate_clears_in_a_clasify_question_is_sent_as_asked() {
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let policy = InputPolicy::ClasifyEvidence {
        content_limit: 32_000,
    };
    for asked in [
        "Does it contain -----BEGIN RSA PRIVATE KEY-----?",
        "Is postgres://localhost:5432/db the default?",
    ] {
        let state = security
            .check_input(
                json!({"content": "plain",
                    "questions": {"q": {"type": "noul", "instructions": asked}}}),
                policy,
            )
            .unwrap_or_else(|issues| panic!("{asked:?}: {issues:?}"));
        assert_eq!(state["questions"]["q"]["instructions"], asked);
    }
}

/// Clasify evidence may be redacted in place; a credential-looking value
/// in a resource read's `query` would run redacted, so it is rejected.
#[test]
fn a_secret_in_a_clasify_source_is_rejected_and_in_state_redacted() {
    let token = format!("ghp_{}", "a".repeat(36));
    let security = octocode_runtime_base::security::ContentSecurity::new();
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

/// The caller's words the request rules cleared reach the tool as written,
/// in every field a provider reads; the same text as evidence is judged as a
/// file's is.
#[test]
fn clasify_words_the_gate_cleared_stay_as_written() {
    let security = octocode_runtime_base::security::ContentSecurity::new();
    let policy = InputPolicy::ClasifyEvidence {
        content_limit: 32_000,
    };
    for words in [
        "Does it contain -----BEGIN RSA PRIVATE KEY-----?",
        "Does it connect to postgres://localhost:5432/db",
        "Is password: str declared, or token: Token, or api_key: str = None?",
        "Is dispatchToken: string a field?",
    ] {
        let row = json!({
            "content": "plain",
            "background": words,
            "questions": {"q": {"type": "noul", "instructions": words}},
        });
        let checked = security.check_input(row.clone(), policy).expect("admitted");
        assert_eq!(*checked, row, "{words}");
    }
    // Evidence that is an armor line alone names a key and holds none of it:
    // it reaches the provider as written. A body beside it never does.
    let checked = security
        .check_input(
            json_fixture(r#"{"content": "-----BEGIN RSA PRIVATE KEY-----"}"#),
            policy,
        )
        .expect("admitted");
    assert_eq!(checked["content"], json!("-----BEGIN RSA PRIVATE KEY-----"));
    let body = "MIIEowIBAAKCAQEAxyzMIIEowIBAAKCAQEAxyzMIIEowIBAAKCAQEAxyzMIIEowIB";
    let checked = security
        .check_input(
            json!({"content": format!("-----BEGIN RSA PRIVATE KEY-----\n{body}\n-----END RSA PRIVATE KEY-----")}),
            policy,
        )
        .expect("redacted in place");
    let content = checked["content"].as_str().expect("content");
    assert!(!content.contains(body), "{content}");
    assert!(
        content.starts_with("-----BEGIN RSA PRIVATE KEY-----\n")
            && content.ends_with("\n-----END RSA PRIVATE KEY-----"),
        "{content}"
    );
}
