//! The walk's exact spans become one placeholder edit each, and those edits are
//! held against the memory budget before they are built: a source the walk
//! kept within its budget is served within the memory budget or refused whole,
//! never served from edits the budget never saw.
use super::super::error::ReadError;
use super::super::masks::Judged;
use super::super::sealed::Edit;
use super::super::stream::stream_pass;
use super::*;
use octocode_engine::security::SecretSpan;
use octocode_io::memo::MemoryBudget;

fn roomy() -> &'static MemoryBudget {
    crate::tools::page_memo::test_budget!(256 * 1024 * 1024)
}
/// Twice what the walk admitted: the spans' own 16 bytes each, and no more.
fn walk_sized_budget() -> &'static MemoryBudget {
    crate::tools::page_memo::test_budget!(walk_sized())
}

/// Secret objects, one value each, past the whole-file ceiling.
fn secret_values() -> (String, usize) {
    let mut text = String::from(r#"{"items":["#);
    let mut values = 0;
    while text.len() < past_whole() {
        text.push_str(&format!(
            r#"{{"data":{{"k":"v{values:06}"}},"kind":"Secret"}},"#
        ));
        values += 1;
    }
    text.push_str("{\"kind\":\"ConfigMap\"}]}\n");
    (text, values)
}

fn values() -> usize {
    secret_values().1
}

fn walk_sized() -> usize {
    2 * values() * std::mem::size_of::<std::ops::Range<usize>>()
}

/// The edit one value costs is more than the span the walk kept for it, so
/// a budget sized to the walk cannot hold the edits.
#[test]
fn an_edit_costs_more_than_the_span_it_comes_from() {
    let placeholder = octocode_engine::security::redaction_placeholder(
        octocode_engine::security::SecretForm::SecretInJson.pattern(),
    );
    assert!(
        std::mem::size_of::<Edit>() + placeholder.len()
            > 2 * std::mem::size_of::<std::ops::Range<usize>>(),
    );
    assert!(std::mem::size_of::<SecretSpan>() >= std::mem::size_of::<std::ops::Range<usize>>());
}

/// With room the walk's spans place every value, and the pass serves it.
#[test]
fn the_walk_fits_and_a_roomy_budget_serves_the_source() {
    let (text, values) = secret_values();
    let file = temp(text.as_bytes());
    super::super::set_memory(Some(roomy()));
    let pass = stream_pass(
        &opened(file.path()),
        file.path(),
        &[Want::lines(1, 1)],
        &crate::security::ContentSecurity::with_limits(LIMITS),
        &NeverCancel,
    );
    super::super::set_memory(None);
    let pass = pass.map_err(|error| error.to_string()).expect("served");
    let handle = opened(file.path());
    let Ok(Judged::Exact(spans)) = pass.proven.judged(&handle, &NeverCancel) else {
        panic!("the walk keeps the source's structure");
    };
    assert_eq!(spans.len(), values);
    assert_eq!(roomy().used(), 0, "the edits' room is given back");
}

/// A budget that holds the walk's spans but not the edits built from them
/// refuses the read whole, with the walk's memory-budget reason.
#[test]
fn edits_the_budget_cannot_hold_refuse_the_read_whole() {
    let (text, _) = secret_values();
    let file = temp(text.as_bytes());
    super::super::set_memory(Some(walk_sized_budget()));
    let pass = stream_pass(
        &opened(file.path()),
        file.path(),
        &[Want::lines(1, 1)],
        &crate::security::ContentSecurity::with_limits(LIMITS),
        &NeverCancel,
    );
    let page = fetch(
        &file,
        &query(serde_json::json!({"path":file.path(),"ranges":[{"startLine":1,"endLine":1}]})),
        None,
    );
    super::super::set_memory(None);
    assert!(matches!(pass.err(), Some(ReadError::Refused)));
    assert_eq!(page.status, ReadStatus::Error, "{page:?}");
    assert!(page.content.is_none(), "no partial view");
    assert!(
        page.partial_reasons
            .contains(&PartialReason::SecurityWalkMemoryBudget),
        "{page:?}"
    );
    assert_eq!(page.terminal_limit, Some(true));
    assert!(!format!("{page:?}").contains("v0000"));
    assert_eq!(walk_sized_budget().used(), 0);
}

/// Plain text, a token on every line, past the whole-file ceiling.
fn token_lines() -> String {
    let mut text = String::new();
    while text.len() < past_whole() {
        text.push_str("ghp_abcdefghijklmnopqrstuvwxyz0123456789\n");
    }
    text
}

/// The scanner's own edits are held against the memory budget as the pass
/// records them: a plain source of dense hits that the budget cannot hold is
/// refused whole, and a roomy budget serves it and gets every byte back.
#[test]
fn scanner_edits_the_budget_cannot_hold_refuse_the_read_whole() {
    let text = token_lines();
    let file = temp(text.as_bytes());
    let pass = |budget: &'static MemoryBudget| {
        super::super::set_memory(Some(budget));
        let pass = stream_pass(
            &opened(file.path()),
            file.path(),
            &[Want::lines(1, 1)],
            &crate::security::ContentSecurity::with_limits(LIMITS),
            &NeverCancel,
        );
        super::super::set_memory(None);
        pass
    };
    let tight = crate::tools::page_memo::test_budget!(4 * 1024);
    assert!(matches!(pass(tight).err(), Some(ReadError::Refused)));
    assert_eq!(tight.used(), 0, "a refused read holds nothing");
    let roomy = roomy();
    assert!(pass(roomy).is_ok(), "a roomy budget serves the source");
    assert_eq!(roomy.used(), 0, "the edits' room is given back");
}
