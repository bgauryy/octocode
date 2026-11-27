use super::*;

#[test]
fn keywords_never_carry_operators_or_qualifiers_out_of_their_term() {
    assert_eq!(quote_search_keyword("fix login"), "\"fix login\"");
    assert_eq!(quote_search_keyword("already"), "already");
    assert_eq!(quote_search_keyword("\"exact phrase\""), "\"exact phrase\"");
    assert_eq!(quote_search_keyword("\"hello\" NOT"), r#""\"hello\" NOT""#);
    assert_eq!(
        quote_search_keyword("x\" OR is:public"),
        r#""x\" OR is:public""#
    );
    // JSON text keeps its quotes: GitHub matches `"name": "ws"` only so.
    assert_eq!(
        quote_search_keyword("\"name\": \"ws\""),
        r#""\"name\": \"ws\"""#
    );
    assert_eq!(quote_search_keyword("repo:other/x"), "\"repo:other/x\"");
    assert_eq!(quote_search_keyword("-excluded"), "\"-excluded\"");
    assert_eq!(quote_search_keyword("NOT"), "\"NOT\"");
    assert_eq!(quote_search_keyword("trail\\"), r#""trail\\""#);
    assert_eq!(quote_search_keyword("\"\""), "");
}

#[test]
fn qualifier_values_stay_one_term() {
    assert_eq!(qualifier_value("rust"), "rust");
    assert_eq!(qualifier_value("Common Lisp"), "\"Common Lisp\"");
    assert_eq!(qualifier_value("\"a b\""), "\"a b\"");
    assert_eq!(qualifier_value("\"a\" OR b"), r#""\"a\" OR b""#);
    assert_eq!(qualifier_value("x)"), "\"x)\"");
}

#[test]
fn person_names_reject_syntax() {
    for bad in ["octocat OR is:public", "a:b", "a\"b", "", "-x", "a b"] {
        assert!(validate_person_name("author", bad).is_err(), "{bad}");
    }
    for good in [
        "octocat",
        "under_score",
        "dependabot[bot]",
        "app/dependabot",
        "dev+x@example.com",
    ] {
        assert!(validate_person_name("author", good).is_ok(), "{good}");
    }
}
