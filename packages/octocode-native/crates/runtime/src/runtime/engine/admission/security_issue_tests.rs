use super::*;

/// A lossy lead's note names what it drops, in the caller's row, once:
/// no static caution about what a drop may change.
#[test]
fn a_lossy_lead_note_names_only_the_dropped_paths() {
    let lead = RepairLead {
        lead: Value::Null,
        dropped: vec!["queries.0.limit".into(), "bogusTop".into()],
        approximate: Vec::new(),
    };
    assert_eq!(
        lead.note(Some(2)).as_deref(),
        Some("The repair lead drops queries[2].limit, bogusTop.")
    );
}

/// Clasify evidence may be redacted in place; a credential-looking value
/// in a resource read's `query` would run redacted, so it is rejected.
#[test]
fn a_secret_in_a_clasify_source_is_rejected_and_in_state_redacted() {
    let token = format!("ghp_{}", "a".repeat(36));
    let security = crate::security::ContentSecurity::new();
    let policy = InputPolicy::ClasifyEvidence {
        state_limit: 32_000,
    };
    let state = security
        .check_input(json!({"state":{"held":token}}), policy)
        .expect("held state is redacted in place");
    assert!(!state.to_string().contains(&token), "{state:?}");
    let issues = security
        .check_input(
            json!({"source":{"tool":"localFetch","query":{"path":token}}}),
            policy,
        )
        .expect_err("a source locator cannot be silently redacted");
    let paths = issues
        .iter()
        .map(|issue| issue.path.join("."))
        .collect::<Vec<_>>();
    assert_eq!(paths, ["source.query.path"], "{issues:?}");
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
