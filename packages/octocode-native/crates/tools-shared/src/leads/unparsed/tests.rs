//! The unparsed record: the warning's count and its `skippedPaths` entries
//! name the same files.
use super::*;

const READER: Reader = Reader {
    grammar: "test grammar",
    unread: ["it was not read", "they were not read"],
};

/// The count a notice opens with (`3 files skipped: …`).
fn counted(notice: Option<String>) -> usize {
    notice
        .as_deref()
        .and_then(|text| text.split_whitespace().next())
        .and_then(|count| count.parse().ok())
        .unwrap_or_default()
}

/// Beside a grammar that reads `ts` names, the warning counts the untyped
/// files and the `ts`-typed ones, each an entry by its type; a file of
/// another known type stays a count. With no grammar read every file is an
/// entry. A file recorded twice is one file. Past [`NAMED_EXAMPLES`] the
/// warning names only examples while the entries list every file.
#[test]
fn the_warning_and_the_entries_count_the_same_files() {
    let ts = known_type(Path::new("x.ts")).expect("ts is a known type");
    let json = known_type(Path::new("x.json")).expect("json is a known type");
    let mut found = Unparsed::default();
    let mut record = |name: &str| {
        let path = format!("/w/{name}");
        found.add(Passed::of(Path::new(&path), name.to_owned()).expect("not binary"));
    };
    for index in 0..NAMED_EXAMPLES + 2 {
        record(&format!("notes{index}.ts.in"));
    }
    record("b.ts");
    record("b.ts");
    record("c.json");
    let read = SourceTypes(
        std::sync::OnceLock::from([ts].into_iter().collect::<std::collections::BTreeSet<_>>()),
        None,
    );
    let (notice, counts) = found.disclosed(&READER, Some(&read), None);
    let entries = found.skipped(Some(&read));
    assert_eq!(entries.len(), counted(notice.clone()), "{notice:?}");
    assert_eq!(entries.len(), NAMED_EXAMPLES + 3);
    assert!(notice.is_some_and(|notice| notice.contains(", …")));
    assert_eq!(
        counts.as_ref().and_then(|counts| counts[json].as_u64()),
        Some(1)
    );
    assert!(
        entries
            .iter()
            .all(|entry| { entry.cause == SkippedPathCause::Unparsed && entry.count == 1 })
    );
    let typed = entries
        .iter()
        .find(|entry| entry.path == "/w/b.ts")
        .expect("the ts file is an entry");
    assert_eq!(typed.reason.as_deref(), Some(ts));
    assert!(
        entries
            .iter()
            .filter(|entry| entry.path.ends_with(".ts.in"))
            .all(|entry| entry.reason.is_none())
    );
    assert!(entries.iter().all(|entry| entry.path != "/w/c.json"));

    let (notice, _) = found.disclosed(&READER, None, None);
    let entries = found.skipped(None);
    assert_eq!(entries.len(), counted(notice));
    assert_eq!(entries.len(), NAMED_EXAMPLES + 4);
}
