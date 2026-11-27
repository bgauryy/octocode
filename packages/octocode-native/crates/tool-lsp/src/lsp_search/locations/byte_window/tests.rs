use super::{line_span, raw_byte_of_column, view_offset};

/// A raw byte maps exactly outside the redacted stretch and to its start
/// inside it.
#[test]
fn raw_offsets_map_onto_the_redacted_view() {
    let raw = "aaSECRETbbXcSECRETdd";
    let view = "aa[REDACTED-A]bbXc[REDACTED-B]dd";
    assert_eq!(view_offset(raw, view, 1), Some(1));
    assert_eq!(
        view_offset(raw, view, 4),
        Some(2),
        "inside a secret: its marker"
    );
    assert_eq!(view_offset(raw, view, 10), Some(16), "between two secrets");
    assert_eq!(
        view_offset(raw, view, 19),
        Some(31),
        "after the last secret"
    );
    assert_eq!(view_offset(raw, raw, 5), Some(5));
}

/// A 7 MB minified line seals as localFetch serves it (whole, redacted).
#[test]
fn a_seven_megabyte_line_seals() {
    let dir = octocode_runtime_base::test_support::Fixture::new();
    let path = dir.0.join("bundle.js");
    let line = (0..250_000)
        .map(|i| format!("function f{i}(a){{return a;}}"))
        .collect::<String>();
    assert!(line.len() > 7_000_000, "{}", line.len());
    std::fs::write(&path, format!("{line}\n")).expect("file");
    let sealed = super::seal(
        &path,
        &octocode_runtime_base::security::ContentSecurity::new(),
    )
    .expect("seals");
    assert_eq!(sealed.view.len(), line.len() + 1);
}

#[test]
fn lines_and_columns_select_as_local_fetch_does() {
    assert_eq!(line_span("a\nbc\nd", 1, 1), Some(2..5));
    assert_eq!(line_span("a\nbc\nd", 1, 2), Some(2..6));
    assert_eq!(line_span("a\n", 1, 1), None);
    assert_eq!(raw_byte_of_column("é😀x", 3), "é😀".len());
}

#[test]
fn differing_multibyte_prefixes_preserve_boundaries() {
    let raw = "[REDACTED-😀secret]suffix";
    let view = "[REDACTED-😁]suffix";
    assert_eq!(
        view_offset(raw, view, raw.len() - "suffix".len()),
        Some(view.len() - "suffix".len())
    );
    assert_eq!(view_offset("éx", "êx", "éx".len()), Some(0));
    assert_eq!(view_offset(raw, view, "[REDACTED-😀".len()), Some(0));
}

#[test]
fn invalid_byte_coordinates_are_refused_without_dropping_text() {
    assert_eq!(view_offset("é😀x", "é😀x", 1), None);
    assert_eq!(view_offset("é😀x", "é😀x", usize::MAX), None);
    assert_eq!(view_offset("é😀x", "é😀x", "é😀".len()), Some("é😀".len()));
    assert_eq!(
        line_span("é\r\n😀x\n", 1, 1),
        Some("é\r\n".len().."é\r\n😀x\n".len())
    );
    assert_eq!(line_span("first\nsecond", 1, 0), None);
}
