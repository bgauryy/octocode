use super::*;

#[test]
fn c_style_line_and_block_comments_removed() {
    let src = "int x = 1; // comment\nint y = 2; /* block */ int z;";
    let out = remove_comments(src, &["c-style"]);
    assert!(!out.contains("comment"));
    assert!(!out.contains("block"));
    assert!(out.contains("int x"));
    assert!(out.contains("int z"));
}

#[test]
fn hash_comments_removed_code_preserved() {
    let src = "x = 1 # inline\n# full line\ny = 2";
    let out = remove_comments(src, &["hash"]);
    assert!(!out.contains("inline"));
    assert!(out.contains("x = 1"));
    assert!(out.contains("y = 2"));
}

#[test]
fn comment_markers_inside_strings_preserved() {
    let src = "const url = \"http://x\"; // real comment";
    let out = remove_comments(src, &["c-style"]);
    assert!(
        out.contains("http://x"),
        "string content must survive: '{out}'"
    );
    assert!(!out.contains("real comment"));
}

#[test]
fn unknown_comment_group_returns_content_unchanged() {
    assert_eq!(remove_comments("hello", &["nonexistent-type"]), "hello");
}

#[test]
fn literal_ranges_finds_quoted_strings() {
    let rules = rules_for("c-style").unwrap();
    let content = r#"a = "hello world"; b = 1;"#;
    let ranges = literal_ranges(content, rules);
    assert_eq!(ranges.len(), 1);
    let (start, end) = ranges[0];
    assert_eq!(&content[start..end], "\"hello world\"");
}

#[test]
fn literal_ranges_finds_js_regex_literal() {
    let rules = rules_for("c-style").unwrap();
    let content = "const re = /a\\/b*c/g;";
    let ranges = literal_ranges(content, rules);
    assert_eq!(ranges.len(), 1);
    let (start, end) = ranges[0];
    assert_eq!(&content[start..end], "/a\\/b*c/g");
}

/// An escaped multibyte character inside a regex literal is
/// stepped over whole, so the closing `/` still ends the literal and the
/// following comment is stripped.
#[test]
fn regex_literal_with_escaped_multibyte_char_ends_at_its_slash() {
    let rules = rules_for("c-style").unwrap();
    let content = "const re = /\\é/g; // gone\nkeep();";
    let ranges = literal_ranges(content, rules);
    assert_eq!(ranges.len(), 1);
    let (start, end) = ranges[0];
    assert_eq!(&content[start..end], "/\\é/g");
    let stripped = strip_string_aware_comments(content, rules);
    assert!(!stripped.contains("gone"), "{stripped:?}");
    assert!(stripped.contains("keep();"), "{stripped:?}");
}

#[test]
fn literal_ranges_ignores_content_with_no_strings() {
    let rules = rules_for("hash").unwrap();
    assert!(literal_ranges("x = 1\ny = 2", rules).is_empty());
}

/// The scans skip every byte `construct_starts` leaves unmarked, so no
/// construct may start at one: for every comment group, at every byte
/// of a corpus built from each construct's first bytes and multibyte
/// text, none of the recognisers matches at an unmarked byte.
#[test]
fn no_construct_starts_at_an_unmarked_byte() {
    let corpus = "x = r#\"raw\"# + br\"b\" @\"v\" $@\"i\" @$\"j\" @'h'\n@\"\nhere\n\"@ /re/g \
        // line # hash -- sql ; semi ' \" ` \"\"\" ''' /* block */ <!-- html --> {- hs -} \
        (* ml *) %{ percent } REM rem \u{e9}\u{1f642}\u{4e2d} #! shebang";
    for (group, rules) in COMMENT_GROUPS {
        let starts = construct_starts(rules, true);
        let literal_starts = construct_starts(rules, false);
        for (pos, byte) in corpus.bytes().enumerate() {
            if !corpus.is_char_boundary(pos) {
                continue;
            }
            if !literal_starts[usize::from(byte)] {
                assert!(
                    find_powershell_here_string(corpus, pos).is_none()
                        && find_rust_raw_string(corpus, pos).is_none()
                        && find_csharp_verbatim(corpus, pos).is_none()
                        && find_regex_literal(corpus, pos).is_none()
                        && !effective_delimiters(rules)
                            .iter()
                            .any(|delimiter| corpus[pos..].starts_with(delimiter)),
                    "{group}: a literal starts at unmarked byte {pos}"
                );
            }
            if !starts[usize::from(byte)] {
                assert!(
                    find_block_comment(corpus, pos, rules.block).is_none()
                        && find_line_comment(corpus, pos, rules.line).is_none(),
                    "{group}: a comment starts at unmarked byte {pos}"
                );
            }
        }
    }
}
