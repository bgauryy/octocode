use super::*;

/// A recovered parse is named in block warnings with the lines the
/// parser skipped; a clean parse adds nothing.
#[test]
fn a_recovered_parse_is_named_with_its_skipped_lines() {
    let flow = "// @flow\ntype Dispatch<A> = A => void;\nexport function useState<S>(init: S): [S, Dispatch<S>] {\n  return [init, () => {}];\n}\n";
    let note = recovered_parse(flow, "Hooks.js").expect("Flow types force a recovered parse");
    assert!(note.contains("could not parse lines"), "{note}");
    assert!(recovered_parse(PY, "m.py").is_none());
    let warning = outline_recovery_warning(
        crate::tools::declarations::file_document(flow, "Hooks.js").as_deref(),
        "Hooks.js",
        &[],
    )
    .expect("outline warning");
    assert!(
        warning.starts_with("The parser could not parse lines"),
        "{warning}"
    );
    assert!(warning.contains("Flow types"), "{warning}");
    assert!(
        outline_recovery_warning(
            crate::tools::declarations::file_document(PY, "m.py").as_deref(),
            "m.py",
            &[]
        )
        .is_none()
    );
}

/// Lines the parser skipped because redaction replaced them are named as
/// redacted, never blamed on syntax the grammar lacks; Flow is named only
/// for JavaScript.
#[test]
fn a_parse_broken_by_redaction_names_the_redacted_lines() {
    let rust =
        "fn a() {\n    let key = 1;\n[REDACTED-PRIVATEKEYFRAGMENT]\n    key\n}\n\nfn b() {}\n";
    let document = crate::tools::declarations::file_document(rust, "k.rs");
    let warning =
        outline_recovery_warning(document.as_deref(), "k.rs", &[3]).expect("recovered parse");
    assert!(warning.contains("redacted"), "{warning}");
    assert!(!warning.contains("Flow"), "{warning}");
    let unredacted =
        outline_recovery_warning(document.as_deref(), "k.rs", &[]).expect("recovered parse");
    assert!(!unredacted.contains("Flow"), "{unredacted}");
    assert!(!unredacted.contains("redacted"), "{unredacted}");
}
