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
/// (count-lines), in term order, on both regex engines.
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

/// A single term counts no terms; a multi-term search of any listing view
/// counts them, so the caller knows which terms hit without a second search.
#[test]
fn only_a_multi_term_search_counts_terms() {
    let t = TmpDir::new();
    t.write("a.rs", "fn one() {}\n");
    let mut lines = opts(t.path(), "(?:fn )|(?:pub )");
    lines.terms = Some(vec!["(?:fn )".into(), "(?:pub )".into()]);
    let listed = search(lines).expect("lines");
    assert_eq!(listed.files[0].term_counts, vec![1, 0]);
    let mut single = opts(t.path(), "fn ");
    single.output = SearchOutput::CountMatches;
    single.terms = Some(vec!["fn ".into()]);
    let counted = search(single).expect("single");
    assert!(counted.files.iter().all(|file| file.term_counts.is_empty()));
}

/// A term found only inside another term's match counts as a search of it
/// alone counts it (`rg -o -F` per term): the combined search reports the
/// leftmost of two overlapping matches, and each term still counts its own.
#[test]
fn overlapping_terms_each_count_their_own_matches() {
    let t = TmpDir::new();
    t.write(
        "a.rs",
        "m.insert(\"snapshot\".into(), v);\nlet k = \"snapshot\".into();\nm.insert(\"snapshot\", w);\n",
    );
    let terms = [r#""snapshot"\.into\(\)"#, r#"insert\("snapshot""#];
    for output in [SearchOutput::CountMatches, SearchOutput::Lines] {
        let mut options = opts(t.path(), &terms.join("|"));
        options.output = output;
        options.terms = Some(terms.iter().map(|term| (*term).to_owned()).collect());
        let found = search(options).expect("search ok");
        assert_eq!(found.files[0].term_counts, vec![2, 2], "{output:?}");
    }
    let mut lines = opts(t.path(), &terms.join("|"));
    lines.output = SearchOutput::CountLines;
    lines.terms = Some(terms.iter().map(|term| (*term).to_owned()).collect());
    assert_eq!(
        search(lines).expect("lines").files[0].term_counts,
        vec![2, 2]
    );
}
