use super::*;

fn rows(source: &str, regions: &[Region], indent: Indent) -> Vec<(usize, String)> {
    lay_out(source, regions, indent, crate::minify::grammars::SEPARATORS)
        .into_iter()
        .map(|kept| (kept.line, kept.text))
        .collect()
}

#[test]
fn numbers_come_from_the_source_lines() {
    let source = "a  =  1\n\n   \nb\n";
    assert_eq!(
        rows(source, &[], Indent::Strip),
        [(1, "a = 1\n".to_owned()), (4, "b\n".to_owned())]
    );
}

#[test]
fn a_comment_separates_and_a_multiline_one_leaves_its_breaks() {
    let source = "a/*x*/b /* y\nz */ c\n";
    let comment = |start, end| Region {
        start,
        end,
        kind: RegionKind::Comment,
    };
    assert_eq!(
        rows(source, &[comment(1, 6), comment(8, 17)], Indent::Strip),
        [(1, "a b\n".to_owned()), (2, "c\n".to_owned())]
    );
}

#[test]
fn a_literal_keeps_its_bytes_and_line_endings() {
    let source = "    x = `a  \r\n\r\n   b`  ;  \r\ny\n";
    let start = source.find('`').unwrap();
    let end = source.rfind('`').unwrap() + 1;
    let literal = Region {
        start,
        end,
        kind: RegionKind::Literal,
    };
    assert_eq!(
        rows(source, &[literal], Indent::Strip),
        [
            (1, "x = `a  \r\n".to_owned()),
            (2, "\r\n".to_owned()),
            (3, "   b`;\n".to_owned()),
            (4, "y\n".to_owned()),
        ]
    );
}

#[test]
fn spaces_go_only_where_they_separate_nothing() {
    assert_eq!(
        rows("f( a , b ) ; g (c) { d }\n", &[], Indent::Strip),
        [(1, "f(a,b);g (c) {d}\n".to_owned())]
    );
}

#[test]
fn indentation_stays_only_where_it_is_syntax() {
    let source = "\tif x:\n\t    y\n";
    assert_eq!(
        rows(source, &[], Indent::Keep),
        [(1, "\tif x:\n".to_owned()), (2, "\t    y\n".to_owned())]
    );
    assert_eq!(
        rows(source, &[], Indent::Strip),
        [(1, "if x:\n".to_owned()), (2, "y\n".to_owned())]
    );
}

#[test]
fn without_a_grammar_only_blank_lines_and_trailing_whitespace_go() {
    let kept: Vec<(usize, String)> = whitespace_only("  a  =  1  # c \r\n\n \n\tb")
        .into_iter()
        .map(|kept| (kept.line, kept.text))
        .collect();
    assert_eq!(
        kept,
        [(1, "  a  =  1  # c\n".to_owned()), (4, "\tb".to_owned())]
    );
}

#[test]
fn as_is_rebuilds_the_source_byte_for_byte() {
    for source in ["a  \r\n\r\n  b\t\r\n", "\n\n", "no final newline  ", ""] {
        let rebuilt: String = as_is(source).into_iter().map(|kept| kept.text).collect();
        assert_eq!(rebuilt, source);
    }
}

#[test]
fn a_line_a_backslash_continues_stays_even_when_empty() {
    let source = "#define A \\\n  b \\\n\n\n#define C 1\n";
    let start = source.find('\\').unwrap();
    let end = source.rfind("\\\n").unwrap() + 2;
    let literal = Region {
        start,
        end,
        kind: RegionKind::Literal,
    };
    let kept = rows(source, &[literal], Indent::Strip);
    assert_eq!(kept[2], (3, "\n".to_owned()), "{kept:?}");
    assert_eq!(kept[3].0, 5, "{kept:?}");
}

#[test]
fn the_last_line_keeps_a_missing_line_ending_missing() {
    assert_eq!(rows("a\nb", &[], Indent::Strip)[1], (2, "b".to_owned()));
}
