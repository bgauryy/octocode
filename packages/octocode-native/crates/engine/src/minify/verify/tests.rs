use super::holds_to_lexis;
use crate::minify::grammars::{CSS_LEXIS, SCSS_LEXIS, SEPARATORS};

/// The views the grammar-led check passed, one per shape a grammar read
/// wrong on both sides, fail against the language's lexis; the right
/// views pass.
#[test]
fn the_lexis_catches_what_a_parse_reads_wrong_on_both_sides() {
    let check = |source, view, lexis| holds_to_lexis(source, view, lexis, SEPARATORS);
    let string = ".a { b: url('<s v=\"0 \\\n    4\"/>'); }\n";
    assert!(check(string, ".a {b: url('<s v=\"0 \\\n4\"/>');}\n", &CSS_LEXIS).is_err());
    assert!(
        check(
            string,
            ".a {b: url('<s v=\"0 \\\n    4\"/>');}\n",
            &CSS_LEXIS
        )
        .is_ok()
    );
    let slashes = "@use \"q\" as *;\n/////\n%h1 {}\n";
    assert!(check(slashes, "@use \"q\" as *;\n/\n%h1 {}\n", &SCSS_LEXIS).is_err());
    assert!(check(slashes, "@use \"q\" as *;\n%h1 {}\n", &SCSS_LEXIS).is_ok());
    let double = "// not a comment\n* { b: c; }\n";
    assert!(check(double, "* {b: c;}\n", &CSS_LEXIS).is_err());
    assert!(check(double, "// not a comment\n* {b: c;}\n", &CSS_LEXIS).is_ok());
    assert!(check(double, "* {b: c;}\n", &SCSS_LEXIS).is_ok());
}

#[test]
fn the_lexis_lets_a_comment_or_space_go_only_where_no_token_changes() {
    let check = |source, view| holds_to_lexis(source, view, &CSS_LEXIS, SEPARATORS);
    assert!(check("a/**/b", "a b").is_err());
    assert!(check("a/**/b", "ab").is_err());
    assert!(check("a/**/b", "a/**/b").is_ok());
    assert!(check(".a/**/.b", ".a .b").is_err());
    assert!(check(".a/**/.b", ".a/**/.b").is_ok());
    assert!(check(".a/**/ .b", ".a .b").is_ok());
    assert!(check(".\\61 /**/.b", ".\\61  .b").is_err());
    assert!(check("a  ,  b", "a,b").is_ok());
    assert!(check(".a :hover", ".a:hover").is_err());
    assert!(check(".a:hover", ".a :hover").is_err());
    assert!(check("a", "a b").is_err());
}
