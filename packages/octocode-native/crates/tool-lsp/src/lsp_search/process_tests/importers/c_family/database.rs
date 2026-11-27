//! What a compile database decides: flags alone list no files, and a finished
//! index speaks only for the files its database lists.
use super::*;
use octocode_test_support::json_fixture;

/// Flags alone list no files, so no index can come of them: the database is what
/// is missing, whatever the flags did for the files the row did read.
#[test]
fn a_row_with_flags_and_no_database_names_the_missing_database() {
    let row = row_of("references", &[(NO_DATABASE, "-xc\n")]);
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["noCompileDatabase"]"#),
        "{row}"
    );
    assert_eq!(
        row["coverage"]["textOnlyFiles"], 0,
        "the text check is stated: {row}"
    );
    assert!(
        !row.to_string().contains("retry"),
        "no index is being built, so no retry helps: {row}"
    );
    let text_leads = row["related"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|call| call["tool"] == "localSearch")
        .count();
    assert_eq!(text_leads, 1, "one text check: {row}");
    assert_eq!(
        row["lsp"]["analysisContext"]["compilation"]["configuration"], "found",
        "{row}"
    );
    let row = row_of("references", &[]);
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["serverUnanswered"]"#),
        "{row}"
    );
}

/// A finished index speaks only for what its database lists: `main.c` is not
/// listed, so it is opened and answered, and with an index behind it the row
/// claims no partiality.
#[test]
fn a_file_the_database_does_not_list_is_opened_and_answered() {
    let index = [(".cache/clangd/index/lib.c.0123ABCD.idx", "x")];
    let pages = pages_with("clangd", &["--background-index"], "callers", &index);
    let row = &pages[0];
    assert_eq!(callers_of(row), ["caller_fn"], "{row}");
    assert_ne!(row["isPartial"], true, "{row}");
    assert!(row["coverage"].get("importerScan").is_none(), "{row}");
}

/// clangd with no compile database parses a header opened alone as another
/// language, so the header's declaration is another identity: no references
/// answer lists it, and its own definition is itself. The declaration the
/// anchor's unit resolves names it, so the references list the header's
/// declaration beside the definition, both as declarations.
#[test]
fn a_header_declaration_is_listed_without_a_database() {
    let row = row_of(
        "references",
        &[(NO_DATABASE, "-xc\n"), ("header-alone", "")],
    );
    let header = row["files"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|group| {
            group["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("lib.h"))
        })
        .unwrap_or_else(|| panic!("the header's declaration is listed: {row}"));
    assert_eq!(header["matches"][0]["role"], "declaration", "{row}");
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["noCompileDatabase"]"#),
        "{row}"
    );
}
