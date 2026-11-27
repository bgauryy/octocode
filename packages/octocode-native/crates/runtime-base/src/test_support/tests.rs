//! The fixtures' own tests: what [`super::production_text`] keeps and
//! blanks, and the page-walk checks.
use super::{checks, production_text};
use octocode_test_support::json_fixture;
use serde_json::Value;

#[test]
fn test_items_are_blanked_and_production_after_them_stays() {
    let source = "fn a() {}\n\
        #[cfg(test)]\n\
        fn only_in_tests() { let x = \"}\"; }\n\
        fn b() { let _ = '\\''; let _ = '{'; }\n\
        #[cfg(test)]\n\
        use helper::{x, y};\n\
        #[cfg(test)]\n\
        impl<A, B> Probe for Pair<A, B> where A: Clone, B: Copy { fn p() {} }\n\
        #[cfg(all(test, unix))]\n\
        mod unix_tests { fn t() { r#\"} {\"#; } }\n\
        fn c<'a>(x: &'a str) {\n    #[cfg(test)]\n    { note(1); }\n    #[cfg(test)]\n    note(2);\n    used();\n}\n\
        #[cfg(test)]\n\
        mod tests;\n\
        #[cfg(not(test))]\n\
        fn real() {}\n\
        #[cfg(test)]\n\
        thread_local! { static T: u8 = 0; }\n\
        fn d() {}\n\
        #[cfg(test)]\n\
        mod tests {\n    fn e() {}\n}\n\
        fn after_tests() {}\n";
    let text = production_text(source);
    assert_eq!(text.lines().count(), source.lines().count(), "lines kept");
    for kept in [
        "fn a() {}",
        "fn b() { let _ = '\\''; let _ = '{'; }",
        "fn c<'a>",
        "used();",
        "#[cfg(not(test))]",
        "fn real() {}",
        "fn d() {}",
        "fn after_tests() {}",
    ] {
        assert!(text.contains(kept), "{kept} kept:\n{text}");
    }
    for blanked in [
        "only_in_tests",
        "helper",
        "Probe",
        "unix_tests",
        "note(",
        "mod tests",
        "thread_local",
        "fn e()",
        "cfg(test)",
        "all(test",
    ] {
        assert!(!text.contains(blanked), "{blanked} blanked:\n{text}");
    }
    // A blanked item keeps every later byte on its line.
    let line = |text: &str, needle: &str| text.lines().position(|line| line.contains(needle));
    assert_eq!(
        line(&text, "fn after_tests"),
        line(source, "fn after_tests")
    );
}

#[test]
fn test_attributes_blank_their_function_and_others_keep_it() {
    for attribute in [
        "#[test]",
        "#[tokio::test]",
        "#[tokio::test(flavor = \"multi_thread\")]",
        "#[cfg(test)]",
    ] {
        let source = format!("{attribute}\nfn probe() {{ run(); }}\nfn kept() {{}}\n");
        let text = production_text(&source);
        assert!(!text.contains("probe") && !text.contains("run()"), "{text}");
        assert!(text.contains("fn kept() {}"), "{text}");
    }
    for attribute in ["#[cfg(not(test))]", "#[must_use]", "#[expect(dead_code)]"] {
        let source = format!("{attribute}\nfn probe() {{ run(); }}\n");
        assert_eq!(production_text(&source), source);
    }
}

#[test]
fn attributes_in_strings_and_comments_gate_nothing() {
    let source = "// #[cfg(test)]\nfn a() {}\nconst S: &str = \"#[cfg(test)]\";\nfn b() {}\n";
    assert_eq!(production_text(source), source);
    let whole = "#![cfg(test)]\nfn a() {}\n";
    assert_eq!(production_text(whole).trim(), "");
}

/// A note's `next.<channel>` promise is checked against the row as built
/// and as served: the named call must be there.
#[test]
fn a_note_that_names_a_next_call_needs_it() {
    let row = |next: Value| {
        serde_json::json!({"results": [{"data": {
            "warnings": ["The list stopped early; `next.nextPage` reads on."],
            "next": next,
        }}]})
    };
    checks::assert_related_promise_has_calls(
        "localSearch",
        &row(json_fixture(
            r#"{
                "nextPage": {"tool": "localSearch", "input": {"cursor": "c"}}
            }"#,
        )),
    );
    // `data.next.x` and a bare `next.` name no channel of the note's own row.
    checks::assert_related_promise_has_calls(
        "localSearch",
        &json_fixture(
            r#"{
                "results": [
                    {
                        "data": {"warnings": ["see data.next.nextPage, or next. alone"]}
                    }
                ]
            }"#,
        ),
    );
    let missing = std::panic::catch_unwind(|| {
        checks::assert_related_promise_has_calls("localSearch", &row(serde_json::json!({})));
    });
    assert!(
        missing.is_err(),
        "a promised next call that is absent fails"
    );
}
