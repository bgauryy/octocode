use super::*;

#[test]
fn dollar_underscore_letters_digits_and_marks_continue_an_identifier() {
    for ch in [
        'a', 'Z', '0', '_', '$', 'é', 'ж', '\u{0301}', '\u{200D}', '℘', '·',
    ] {
        assert!(is_ident_char(ch), "{ch:?}");
    }
    for ch in [' ', '.', ':', '-', '(', '#', '@', '—', '\u{00A0}'] {
        assert!(!is_ident_char(ch), "{ch:?}");
    }
}

/// `$foo` is one identifier in JS/TS, so `foo` is not a whole word in it.
#[test]
fn whole_word_needs_a_non_identifier_neighbor_on_identifier_ends() {
    let words = |text: &str, word: &str| whole_word_starts(text, word).collect::<Vec<_>>();
    assert_eq!(words("foo(1) + foo", "foo"), [0, 9]);
    assert!(words("$foo", "foo").is_empty());
    assert!(words("foo$", "foo").is_empty());
    assert!(words("_foo xfoo foo1 éfoo", "foo").is_empty());
    assert_eq!(words("—foo—", "foo"), ["—".len()]);
    assert_eq!(words("a$store.b", "$store"), Vec::<usize>::new());
    assert_eq!(words("x = $store;", "$store"), [4]);
    // An end of the word that is itself punctuation needs no boundary.
    assert_eq!(words("appState.f()", ".f"), [8]);
    assert!(words("", "f").is_empty());
    assert!(words("f", "").is_empty());
}

/// Bytes that are not UTF-8 next to a match count as identifier bytes: the
/// match is not proven whole.
#[test]
fn undecodable_neighbor_bytes_are_not_boundaries() {
    assert!(is_whole_word(b"(foo)", 1, 4));
    assert!(!is_whole_word(b"\x80foo", 1, 4));
    assert!(!is_whole_word(b"foo\xC3", 0, 3));
    assert!(is_whole_word("—foo".as_bytes(), "—".len(), "—".len() + 3));
    assert!(!is_whole_word("éfoo".as_bytes(), "é".len(), "é".len() + 3));
}

#[test]
fn word_pattern_bounds_only_identifier_edges() {
    assert_eq!(
        word_pattern("greet"),
        r"(?:^|[^\w\p{ID_Continue}$])greet(?:[^\w\p{ID_Continue}$]|$)"
    );
    assert_eq!(
        word_pattern("$store"),
        r"(?:^|[^\w\p{ID_Continue}$])\$store(?:[^\w\p{ID_Continue}$]|$)"
    );
    assert_eq!(word_pattern(".f"), r"\.f(?:[^\w\p{ID_Continue}$]|$)");
}

/// The regex form and the predicate give the same answer on every line.
#[test]
fn word_pattern_agrees_with_the_predicate() {
    let cases = [
        "foo",
        "$foo",
        "foo$",
        "_foo",
        "xfoo",
        "foo1",
        "éfoo",
        "—foo—",
        "a.foo()",
        "foo bar",
        "a $foo b",
        "(foo)",
        "\u{200D}foo",
    ];
    let pattern = regex::Regex::new(&format!("(?m){}", word_pattern("foo"))).expect("pattern");
    for text in cases {
        assert_eq!(
            pattern.is_match(text),
            whole_word_starts(text, "foo").next().is_some(),
            "{text:?}"
        );
    }
}

struct AllowAll;

impl crate::search::text_search::TextSearchPathFilter for AllowAll {
    fn allows(&self, _: &std::path::Path, _: bool) -> bool {
        true
    }
}

/// The files-with-match text search (the lexical scope scan) finds `foo`
/// in exactly the files where the predicate finds a whole word.
#[test]
fn file_search_with_word_pattern_agrees_with_the_predicate() {
    let root = crate::test_support::TmpDir::new();
    let cases = [
        "foo",
        "$foo",
        "foo$",
        "_foo",
        "xfoo",
        "éfoo",
        "—foo—",
        "a.foo()",
        "x\n  $foo\n",
        "x\nfoo\n",
        "a $foo b",
    ];
    for (index, text) in cases.iter().enumerate() {
        std::fs::write(root.join(format!("{index}.ts")), text).expect("write case");
    }
    let found = crate::search::text_search::search_cancellable(
        crate::types::TextSearchOptions {
            path: root.to_string_lossy().into_owned(),
            pattern: word_pattern("foo"),
            output: crate::types::SearchOutput::FilesWithMatch,
            ..Default::default()
        },
        std::sync::Arc::new(AllowAll),
        &|| false,
    )
    .expect("search");
    let mut matched: Vec<String> = found
        .files
        .iter()
        .map(|file| {
            std::path::Path::new(&file.path)
                .file_name()
                .expect("name")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    matched.sort();
    let mut expected: Vec<String> = cases
        .iter()
        .enumerate()
        .filter(|(_, text)| whole_word_starts(text, "foo").next().is_some())
        .map(|(index, _)| format!("{index}.ts"))
        .collect();
    expected.sort();
    assert_eq!(matched, expected);
    assert_eq!(expected.len(), 4, "{expected:?}");
}
