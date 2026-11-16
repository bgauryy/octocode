//! A file whose own bytes prove the text its judge serves holds no match is
//! not judged; every other file is searched as that text, as before.

use super::*;
use crate::types::SourceJudge;
use std::sync::atomic::AtomicUsize;

/// What the fake judge hides, and what it writes in its place.
const HIDDEN: &str = "SECRET123";
const WRITTEN: &str = "[HID]";

/// A judge that serves each file with [`HIDDEN`] written as [`WRITTEN`],
/// counting the files it judged.
fn judge(judged: &Arc<AtomicUsize>) -> SourceJudge {
    let count = Arc::clone(judged);
    SourceJudge {
        serve: Arc::new(
            move |_: &Path, file: &std::fs::File, _: &(dyn Fn() -> bool + Sync)| {
                use std::io::Read as _;
                count.fetch_add(1, Ordering::Relaxed);
                let mut text = String::new();
                (&*file).read_to_string(&mut text)?;
                Ok(text
                    .contains(HIDDEN)
                    .then(|| text.replace(HIDDEN, WRITTEN).into_bytes()))
            },
        ),
        writes: Arc::new(|literal: &str, fold: bool| {
            let (literal, written) = if fold {
                (literal.to_ascii_uppercase(), WRITTEN.to_owned())
            } else {
                (literal.to_owned(), WRITTEN.to_owned())
            };
            literal.contains(['[', ']']) || written.contains(&literal)
        }),
    }
}

fn root() -> TmpDir {
    let root = TmpDir::new();
    root.write("plain.txt", "alpha beta\n");
    root.write("hidden.txt", "key SECRET123 here\n");
    root.write("joined.txt", "fooSECRET123\n");
    root.write("binary.bin", "\0\x01binary SECRET123 alpha\n");
    root
}

/// The files `pattern` matches with `fields` set, and how many files were judged.
fn run(
    root: &TmpDir,
    pattern: &str,
    fields: impl FnOnce(&mut TextSearchOptions),
) -> (Vec<String>, usize) {
    let judged = Arc::new(AtomicUsize::new(0));
    let mut options = opts(root.path(), pattern);
    options.output = SearchOutput::FilesWithMatch;
    options.sort = SearchSort::Path;
    options.judge = Some(judge(&judged));
    fields(&mut options);
    let result = search(options).expect("search");
    let mut files = result
        .files
        .iter()
        .map(|file| {
            Path::new(&file.path)
                .file_name()
                .expect("name")
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    files.sort();
    (files, judged.load(Ordering::Relaxed))
}

#[test]
fn a_file_whose_bytes_hold_no_match_is_not_judged() {
    let root = root();
    assert_eq!(run(&root, "defects-queue", |_| {}), (vec![], 0));
    // Only the file whose bytes match is judged, and it matches as served.
    assert_eq!(run(&root, "alpha", |_| {}), (vec!["plain.txt".into()], 1));
    for view in [
        SearchOutput::CountLines,
        SearchOutput::CountMatches,
        SearchOutput::Lines,
    ] {
        let (files, judged) = run(&root, "beta", |options| options.output = view);
        assert_eq!((files, judged), (vec!["plain.txt".into()], 1), "{view:?}");
    }
}

#[test]
fn a_hidden_value_never_matches() {
    let root = root();
    for pattern in [HIDDEN, "SECRET1", "CRET12"] {
        let (files, judged) = run(&root, pattern, |_| {});
        assert!(files.is_empty(), "{pattern}: {files:?}");
        // Both text files whose bytes hold it were judged.
        assert_eq!(judged, 2, "{pattern}");
    }
    let (files, _) = run(&root, "secret123", |options| {
        options.case = CaseRule::Insensitive;
    });
    assert!(files.is_empty(), "{files:?}");
}

#[test]
fn a_pattern_that_can_match_written_text_judges_every_file() {
    let root = root();
    for pattern in ["HID", r"\[HID", "hid"] {
        let (files, judged) = run(&root, pattern, |options| {
            options.syntax = PatternSyntax::Regex;
            options.case = CaseRule::Smart;
        });
        assert_eq!(
            files,
            vec!["hidden.txt".to_owned(), "joined.txt".to_owned()],
            "{pattern}"
        );
        assert_eq!(judged, 4, "{pattern}");
    }
}

/// A match next to written text that its bytes do not hold: a word boundary
/// the written text makes, a regex whose prefix the bytes hold.
#[test]
fn a_match_that_takes_its_context_from_written_text_is_found() {
    let root = root();
    let (files, judged) = run(&root, "foo", |options| options.whole_word = Some(true));
    assert_eq!((files, judged), (vec!["joined.txt".into()], 1));
    let (files, judged) = run(&root, r"foo.*D\]", |options| {
        options.syntax = PatternSyntax::Regex;
    });
    // Every match starts with `foo`, which takes no written text: only the
    // file whose bytes hold it is judged, and its served text matches.
    assert_eq!((files, judged), (vec!["joined.txt".into()], 1));
    let (files, judged) = run(&root, r"fo+\b", |options| {
        options.syntax = PatternSyntax::Regex;
    });
    assert_eq!((files, judged), (vec!["joined.txt".into()], 1));
}

#[test]
fn a_files_without_view_or_inverted_search_judges_every_file() {
    let root = root();
    let (_, judged) = run(&root, "defects-queue", |options| {
        options.output = SearchOutput::FilesWithoutMatch;
    });
    assert_eq!(judged, 4);
    let (_, judged) = run(&root, "defects-queue", |options| {
        options.invert_match = Some(true);
    });
    assert_eq!(judged, 4);
}

#[test]
fn a_listed_count_judges_only_files_whose_bytes_match() {
    let root = root();
    let judged = Arc::new(AtomicUsize::new(0));
    let base = |pattern: &str| {
        let mut options = opts(root.path(), pattern);
        options.output = SearchOutput::FilesWithMatch;
        options.judge = Some(judge(&judged));
        options
    };
    let (missing, alpha, hidden) = (base("defects-queue"), base("alpha"), base(HIDDEN));
    let files = ["plain.txt", "hidden.txt", "joined.txt"].map(|name| root.join(name));
    let counts = count_listed(&[&missing, &alpha, &hidden], &files, &|| false).expect("count");
    assert_eq!(counts, vec![0, 1, 0]);
    // One judgment per file whose bytes some matcher matched.
    assert_eq!(judged.load(Ordering::Relaxed), 3);
}
