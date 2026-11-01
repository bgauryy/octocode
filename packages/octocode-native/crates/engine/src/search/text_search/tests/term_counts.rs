//! Per-term counts of a count view over an alternation of terms.
use super::*;

fn counted(output: SearchOutput, syntax: PatternSyntax) -> Vec<(String, Vec<u32>)> {
    let t = TmpDir::new();
    t.write("a.rs", "fn one() {}\npub fn two() {}\npub struct S;\n");
    t.write("b.rs", "struct T;\n");
    let mut options = opts(t.path(), "(?:fn )|(?:pub )|(?:struct )");
    options.syntax = syntax;
    options.output = output;
    options.terms = Some(vec![
        "(?:fn )".into(),
        "(?:pub )".into(),
        "(?:struct )".into(),
    ]);
    let mut files = search(options)
        .expect("search ok")
        .files
        .into_iter()
        .map(|file| {
            let name = Path::new(&file.path)
                .file_name()
                .expect("name")
                .to_string_lossy()
                .into_owned();
            (name, file.term_counts)
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

/// Each term's matches (count-matches) or the lines holding it
/// (count-lines), in term order, on both regex engines; they sum to the
/// file's match count, each match attributed once.
#[test]
fn a_count_view_counts_each_term() {
    for syntax in [PatternSyntax::Regex, PatternSyntax::Pcre2] {
        assert_eq!(
            counted(SearchOutput::CountMatches, syntax),
            [
                ("a.rs".into(), vec![2, 2, 1]),
                ("b.rs".into(), vec![0, 0, 1])
            ],
            "{syntax:?}"
        );
        assert_eq!(
            counted(SearchOutput::CountLines, syntax),
            [
                ("a.rs".into(), vec![2, 2, 1]),
                ("b.rs".into(), vec![0, 0, 1])
            ],
            "{syntax:?}"
        );
    }
}

/// A view that lists lines, or a single term, counts no terms.
#[test]
fn only_a_multi_term_count_view_counts_terms() {
    let t = TmpDir::new();
    t.write("a.rs", "fn one() {}\n");
    let mut lines = opts(t.path(), "(?:fn )|(?:pub )");
    lines.terms = Some(vec!["(?:fn )".into(), "(?:pub )".into()]);
    let listed = search(lines).expect("lines");
    assert!(listed.files.iter().all(|file| file.term_counts.is_empty()));
    let mut single = opts(t.path(), "fn ");
    single.output = SearchOutput::CountMatches;
    single.terms = Some(vec!["fn ".into()]);
    let counted = search(single).expect("single");
    assert!(counted.files.iter().all(|file| file.term_counts.is_empty()));
}
