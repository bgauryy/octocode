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

/// Beside a `skippedPaths` that names each counted file, the warning keeps
/// the count and the cause and names no file a second time; once that list
/// groups its files (past [`NAMED_UNPARSED`]) the warning's examples are the
/// only names, so they stay. A page that lists no paths names examples.
#[test]
fn the_warning_names_no_file_its_skipped_paths_already_name() {
    use crate::leads::skipped::{NAMED_UNPARSED, unparsed_paths};
    let found_of = |files: usize| {
        let mut found = Unparsed::default();
        for index in 0..files {
            let name = format!("notes{index:03}.unknowntype0");
            let path = format!("/w/{name}");
            found.add(Passed::of(Path::new(&path), name).expect("not binary"));
        }
        found
    };
    for files in [1, 2, NAMED_UNPARSED, NAMED_UNPARSED + 1] {
        let found = found_of(files);
        let entries = found.skipped(None);
        let each_named = unparsed_paths(entries.clone(), false)
            .expect("skipped paths")
            .expect("entries")
            .entries
            .len()
            == files;
        assert_eq!(each_named, files <= NAMED_UNPARSED, "{files} files");
        let (beside, _) = found.disclosed_beside_paths(&READER, None, None);
        let (alone, _) = found.disclosed(&READER, None, None);
        let (beside, alone) = (beside.expect("a notice"), alone.expect("a notice"));
        assert_eq!(counted(Some(beside.clone())), files);
        assert!(beside.contains(READER.grammar), "{beside}");
        let names = |notice: &str| {
            entries
                .iter()
                .filter(|entry| {
                    let name = entry.path.rsplit('/').next().expect("a name");
                    notice.contains(name)
                })
                .count()
        };
        assert_eq!(names(&alone), files.min(NAMED_EXAMPLES), "{alone}");
        if each_named {
            assert_eq!(names(&beside), 0, "{beside}");
            assert!(beside.len() < alone.len(), "{beside} against {alone}");
        } else {
            assert_eq!(beside, alone, "a grouped list names no file");
        }
    }
}

/// A name that is no file type is refused as the walk refuses it, and the
/// types of a file name are every type whose globs take it.
#[test]
fn a_file_name_lists_every_type_it_is_of() {
    assert!(select_file_type("rust").is_ok());
    assert!(select_file_type("rs").is_err());
    assert_eq!(file_types_of("x.rs"), ["rust"]);
    let several = file_types_of("CMakeLists.txt");
    assert!(several.len() > 1, "{several:?}");
    assert_eq!(
        known_type(Path::new("CMakeLists.txt")),
        several.first().copied()
    );
    assert!(file_types_of("x.unknowntype0").is_empty());
    for name in file_types_of("x.h") {
        assert!(select_file_type(name).is_ok(), "{name}");
    }
}
