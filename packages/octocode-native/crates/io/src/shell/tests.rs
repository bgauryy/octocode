use super::quote;

#[test]
fn safe_words_stay_bare_and_everything_else_is_one_quoted_word() {
    assert_eq!(quote("src/a-b_c.rs:1@2,3=4+5"), "src/a-b_c.rs:1@2,3=4+5");
    assert_eq!(quote("two words"), "'two words'");
    assert_eq!(quote("it's"), r"'it'\''s'");
    assert_eq!(quote("$HOME;`x`"), "'$HOME;`x`'");
    assert_eq!(quote("a\nb"), "'a\nb'");
}

#[test]
fn an_empty_argument_is_kept_as_an_empty_word() {
    assert_eq!(quote(""), "''");
}
