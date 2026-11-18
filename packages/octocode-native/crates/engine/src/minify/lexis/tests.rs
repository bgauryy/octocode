use super::*;
use crate::minify::grammars::{CSS_LEXIS, SCSS_LEXIS};

fn pieces<'a>(source: &'a str, lexis: &Lexis) -> Vec<(&'a str, Piece)> {
    spans(source, lexis)
        .into_iter()
        .map(|span| (&source[span.start..span.end], span.piece))
        .collect()
}

#[test]
fn strings_run_across_escaped_line_breaks_and_end_at_a_bare_one() {
    let source = "a: 'x \\\n  y' \"b\\\"c\" 'open\nd";
    assert_eq!(
        pieces(source, &CSS_LEXIS),
        [
            ("'x \\\n  y'", Piece::String),
            ("\"b\\\"c\"", Piece::String),
            ("'open", Piece::String),
        ]
    );
}

#[test]
fn a_double_slash_is_a_comment_only_where_the_language_says() {
    let source = "/////\n// c\n.a{b:url(//x/y)}/* d */\n";
    assert_eq!(pieces(source, &CSS_LEXIS), [("/* d */", Piece::Comment)]);
    assert_eq!(
        pieces(source, &SCSS_LEXIS),
        [
            ("/////", Piece::Comment),
            ("// c", Piece::Comment),
            ("/* d */", Piece::Comment),
        ]
    );
    // A quoted url is an ordinary call; a name that only ends in `url` is
    // not one.
    assert_eq!(
        pieces("url( '//a' ) myurl(//b)\n", &SCSS_LEXIS),
        [("'//a'", Piece::String), ("//b)", Piece::Comment)]
    );
}

#[test]
fn a_comment_the_spec_does_not_have_stays_and_a_string_absorbs_overlaps() {
    let region = |start, end, kind| Region { start, end, kind };
    // The grammar's `/` then `////` comment; the spec's `/////` comment.
    let source = "/////\n'a \\\n b' c /* d */";
    let spans = spans(source, &SCSS_LEXIS);
    let regions = vec![
        region(1, 5, RegionKind::Comment),
        region(6, 11, RegionKind::Literal),
        region(17, 24, RegionKind::Comment),
    ];
    assert_eq!(
        agree(source, regions, &spans, &SCSS_LEXIS),
        [
            region(1, 5, RegionKind::Literal),
            region(6, 14, RegionKind::Literal),
            region(17, 24, RegionKind::Comment),
        ]
    );
}
