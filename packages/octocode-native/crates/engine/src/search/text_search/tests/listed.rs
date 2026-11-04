//! A pass over a known file list answers as the walk of those files does.

use super::*;
use crate::search::text_search::listed::reads;

fn probe(
    root: &TmpDir,
    pattern: &str,
    fields: impl FnOnce(&mut TextSearchOptions),
) -> TextSearchOptions {
    let mut options = opts(root.path(), pattern);
    options.output = SearchOutput::FilesWithMatch;
    options.sort = SearchSort::Path;
    options.ignore = crate::types::IgnorePolicy::ALL;
    fields(&mut options);
    options
}

fn listed_files(root: &TmpDir) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir).expect("read") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}

/// Text, mixed case, a late and a leading NUL, a long NUL-free run before a
/// NUL, a BOM, CRLF, an empty file: what decides a walk's verdict on a file.
fn zoo() -> TmpDir {
    let root = TmpDir::new();
    root.write("plain.txt", "alpha beta\nsecond line\n");
    root.write("upper.txt", "ALPHA BETA\n");
    root.write("word.txt", "alphabet soup\n");
    root.write("crlf.txt", "alpha\r\nbeta\r\n");
    root.write("across.txt", "alpha\nbeta\n");
    root.write("empty.txt", "");
    root.write("deep/dir/a.txt", "Alpha\n");
    let binary = |name: &str, bytes: Vec<u8>| {
        fs::write(root.join(name), bytes).expect("binary");
    };
    binary("nul-first.bin", b"\0alpha\n".to_vec());
    binary("header.bin", b"alpha\0more\n".to_vec());
    binary("after-line.bin", b"alpha line\nnext\0tail alpha\n".to_vec());
    binary("miss-before-nul.bin", b"nothing here\n\0alpha\n".to_vec());
    let mut long = "alpha filler\n".repeat(20_000).into_bytes();
    long.extend_from_slice(b"\0alpha\n");
    binary("long-then-nul.bin", long);
    let mut far = "filler line\n".repeat(20_000).into_bytes();
    far.extend_from_slice(b"alpha\n\0");
    binary("far-match.bin", far);
    let utf16: Vec<u8> = [0xFF, 0xFE]
        .into_iter()
        .chain("alpha\n".encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    binary("utf16.txt", utf16);
    root
}

fn walked_count(options: &TextSearchOptions) -> usize {
    search(options.clone()).expect("walk").files.len()
}

#[test]
fn a_listed_pass_counts_what_the_walk_finds_for_each_alternate() {
    let root = zoo();
    let files = listed_files(&root);
    let shapes: Vec<TextSearchOptions> = vec![
        probe(&root, "alpha", |o| {
            o.syntax = PatternSyntax::Literal;
            o.case = CaseRule::Insensitive;
        }),
        probe(&root, "alpha", |o| {
            o.syntax = PatternSyntax::Literal;
            o.case = CaseRule::Sensitive;
        }),
        probe(&root, "a.pha", |o| o.syntax = PatternSyntax::Regex),
        probe(&root, "alpha$", |o| o.syntax = PatternSyntax::Regex),
        probe(&root, "alpha", |o| o.whole_word = Some(true)),
        probe(&root, "alpha", |o| o.invert_match = Some(true)),
        probe(&root, "alpha.beta", |o| {
            o.multiline = Some(true);
            o.multiline_dotall = Some(true);
        }),
        probe(&root, "absent", |_| {}),
    ];
    let walked = shapes.iter().map(walked_count).collect::<Vec<_>>();
    for (first, walked_first) in shapes.iter().zip(&walked) {
        let alone = count_listed(&[first], &files, &|| false).expect("one");
        assert_eq!(alone, [*walked_first], "{}", first.pattern);
        for (second, walked_second) in shapes.iter().zip(&walked) {
            let both = count_listed(&[first, second], &files, &|| false).expect("both");
            assert_eq!(
                both,
                [*walked_first, *walked_second],
                "{} and {}",
                first.pattern,
                second.pattern
            );
        }
    }
}

#[test]
fn two_alternates_share_one_read_and_a_file_past_the_heap_is_streamed_for_each() {
    let root = TmpDir::new();
    root.write("a.txt", "alpha\n");
    let files = listed_files(&root);
    let case = probe(&root, "ALPHA", |o| o.case = CaseRule::Insensitive);
    let regex = probe(&root, "al.ha", |_| {});
    let before = reads();
    assert_eq!(
        count_listed(&[&case, &regex], &files, &|| false).expect("both"),
        [1, 1]
    );
    assert_eq!(reads() - before, 1, "one read per file");
    let before = reads();
    assert_eq!(count_listed(&[&case], &files, &|| false).expect("one"), [1]);
    assert_eq!(reads() - before, 1);

    let big = fs::OpenOptions::new()
        .append(true)
        .open(&files[0])
        .expect("open");
    big.set_len(super::file_search::SEARCH_HEAP_LIMIT_BYTES as u64 + 1)
        .expect("sparse");
    let before = reads();
    assert_eq!(
        count_listed(&[&case, &regex], &files, &|| false).expect("both"),
        [1, 1]
    );
    assert_eq!(reads() - before, 2, "held in neither");
}

#[test]
fn a_cancelled_pass_keeps_what_it_counted() {
    let root = TmpDir::new();
    root.write("a.txt", "alpha\n");
    root.write("b.txt", "alpha\n");
    let files = listed_files(&root);
    let case = probe(&root, "alpha", |_| {});
    assert_eq!(count_listed(&[&case], &files, &|| true).expect("run"), [0]);
    let missing = vec![root.join("gone.txt")];
    assert_eq!(
        count_listed(&[&case], &missing, &|| false).expect("run"),
        [0]
    );
    assert_eq!(count_listed(&[&case], &[], &|| false).expect("run"), [0]);
}

#[test]
fn what_a_list_cannot_answer_is_refused_and_a_bad_pattern_fails() {
    let root = TmpDir::new();
    let with = |fields: fn(&mut TextSearchOptions)| probe(&root, "alpha", fields);
    assert!(listable(&with(|_| {})));
    for options in [
        with(|o| o.syntax = PatternSyntax::Pcre2),
        with(|o| o.output = SearchOutput::Lines),
        with(|o| o.file_contains_all = Some(vec!["x".to_owned()])),
        with(|o| o.file_contains_none = Some(vec!["x".to_owned()])),
    ] {
        assert!(!listable(&options));
        assert!(count_listed(&[&options], &[], &|| false).is_err());
    }
    let broken = probe(&root, "(", |o| o.syntax = PatternSyntax::Regex);
    assert!(count_listed(&[&broken], &[], &|| false).is_err());
}
