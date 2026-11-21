//! Server-written text against the files it is about. All secrets are fake.
use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;
use std::path::Path;

const PASSWORD: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
const CFG: &str = "export const cfg = { user: \"svc\", aws_secret_access_key: \"wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY\" } as const;\n";
const USE: &str = "import { cfg } from \"./cfg\";\nexport const p = cfg.aws_secret_access_key;\n";

/// `raw` judged whole as the file `name`.
fn judged(name: &str, raw: &str) -> Arc<JudgedText> {
    Arc::new(JudgedText::judge(
        raw,
        Path::new(name),
        &ContentSecurity::new(),
    ))
}

/// The placeholder a read of `source` shows where `value` stands.
fn read_placeholder(source: &JudgedText, value: &str) -> String {
    let read = source.redaction().expect("the read redacts").0;
    assert!(!read.contains(value), "the read hides {value}: {read}");
    octocode_engine::security::placeholder_regex()
        .find(read)
        .expect("a placeholder in the read")
        .as_str()
        .to_owned()
}

fn hover(text: &str) -> Value {
    json!({
        "operation": "hover",
        "hover": {"contents": {"kind": "markdown", "value": text}},
    })
}

#[test]
fn a_hover_shows_a_value_of_the_anchor_file_as_a_read_does() {
    let anchor = judged("cfg.ts", CFG);
    let mut row = hover(&format!(
        "\n```typescript\n(property) password: \"{PASSWORD}\"\n```\n"
    ));
    let done = scrub(&mut row, std::slice::from_ref(&anchor), false);
    let shown = row["hover"]["contents"]["value"].as_str().expect("hover");
    assert!(!row.to_string().contains(PASSWORD), "{row}");
    assert_eq!(
        shown,
        format!(
            "\n```typescript\n(property) password: \"{}\"\n```\n",
            read_placeholder(&anchor, PASSWORD)
        )
    );
    assert_eq!(
        done,
        Scrubbed {
            replaced: 1,
            withheld: 0
        }
    );
}

#[test]
fn a_hover_shows_a_value_of_the_file_its_definition_names_as_a_read_does() {
    let (anchor, declared) = (judged("use.ts", USE), judged("cfg.ts", CFG));
    let mut row = hover(&format!("\n```typescript\nconst p: \"{PASSWORD}\"\n```\n"));
    let done = scrub(&mut row, &[anchor, Arc::clone(&declared)], false);
    assert!(!row.to_string().contains(PASSWORD), "{row}");
    assert_eq!(
        row["hover"]["contents"]["value"],
        format!(
            "\n```typescript\nconst p: \"{}\"\n```\n",
            read_placeholder(&declared, PASSWORD)
        )
    );
    assert_eq!(done.replaced, 1, "{row}");
}

#[test]
fn a_diagnostic_message_shows_a_value_of_its_file_as_a_read_does() {
    let source = "export const aws_secret_access_key = \"Tr0ub4dorXK9zQwTr0ub4dorXK9zQwTr0ub4dorX\" as const;\nexport function useIt(p: typeof aws_secret_access_key) { return p; }\nuseIt(\"wrong-Value-9f86d081\");\n";
    let anchor = judged("x.ts", source);
    let mut row = json_fixture(
        r#"{
            "operation": "diagnostics",
            "matches": [
                {
                    "source": "typescript",
                    "severity": "error",
                    "message": "Argument of type '\"wrong-Value-9f86d081\"' is not assignable to parameter of type '\"Tr0ub4dorXK9zQwTr0ub4dorXK9zQwTr0ub4dorX\"'.",
                    "relatedInformation": [
                        {
                            "message": "The expected type comes from \"Tr0ub4dorXK9zQwTr0ub4dorXK9zQwTr0ub4dorX\""
                        }
                    ]
                }
            ]
        }"#,
    );
    scrub(&mut row, std::slice::from_ref(&anchor), false);
    assert!(!row.to_string().contains("Tr0ub4dorXK9"), "{row}");
    let placeholder = read_placeholder(&anchor, "Tr0ub4dorXK9zQwTr0ub4dorXK9zQwTr0ub4dorX");
    // The value the file shows in the clear stays: a read shows it too.
    assert_eq!(
        row["matches"][0]["message"],
        format!(
            "Argument of type '\"wrong-Value-9f86d081\"' is not assignable to parameter of type '\"{placeholder}\"'."
        )
    );
    assert_eq!(
        row["matches"][0]["relatedInformation"][0]["message"],
        format!("The expected type comes from \"{placeholder}\"")
    );
}

#[test]
fn symbol_names_and_details_show_a_value_of_their_file_as_a_read_does() {
    let anchor = judged("cfg.ts", CFG);
    let placeholder = read_placeholder(&anchor, PASSWORD);
    // An outline row, a call-hierarchy node and a type-hierarchy item.
    let mut row = json!({
        "operation": "documentSymbols",
        "matches": [
            {"name": "password", "detail": format!("\"{PASSWORD}\""), "kind": "property"},
            {"symbolName": format!("cfg[\"{PASSWORD}\"]"), "detail": format!("= {PASSWORD}"), "path": "cfg.ts"},
            {"from": {"symbolName": "Cfg", "detail": format!("type Cfg = \"{PASSWORD}\""), "path": "cfg.ts"}},
        ],
    });
    let done = scrub(&mut row, std::slice::from_ref(&anchor), false);
    assert!(!row.to_string().contains(PASSWORD), "{row}");
    assert_eq!(row["matches"][0]["detail"], format!("\"{placeholder}\""));
    assert_eq!(row["matches"][0]["name"], "password");
    assert_eq!(
        row["matches"][1]["symbolName"],
        format!("cfg[\"{placeholder}\"]")
    );
    assert_eq!(row["matches"][1]["detail"], format!("= {placeholder}"));
    assert_eq!(
        row["matches"][2]["from"]["detail"],
        format!("type Cfg = \"{placeholder}\"")
    );
    assert_eq!(done.replaced, 4, "{row}");
}

/// The value's file is not one the row is about (no definition names it):
/// no key was read for it, so type text withholds it.
#[test]
fn type_text_withholds_a_string_literal_no_judged_file_holds() {
    let anchor = judged("use.ts", USE);
    let mut row = hover(&format!(
        "\n```typescript\nconst p: \"{PASSWORD}\" | 'Yr0ub4dor' | \"./cfg\"\n```\nIt's the user's `p`.\n"
    ));
    let done = scrub(&mut row, &[anchor], false);
    assert_eq!(
        row["hover"]["contents"]["value"],
        format!(
            "\n```typescript\nconst p: \"{REDACTED}\" | '{REDACTED}' | \"./cfg\"\n```\nIt's the user's `p`.\n"
        )
    );
    assert_eq!(
        done,
        Scrubbed {
            replaced: 0,
            withheld: 2
        }
    );
    // A message quotes names in prose; a literal is a quote inside a quote.
    let mut row = json!({"matches": [{"message": format!(
        "Type '\"{PASSWORD}\"' is not assignable to type 'number'. \"Literal['Yr0ub4dor']\" is not \"int\"; see `\"other\"`."
    )}]});
    let done = scrub(&mut row, &[judged("use.ts", USE)], false);
    assert_eq!(
        row["matches"][0]["message"],
        format!(
            "Type '\"{REDACTED}\"' is not assignable to type 'number'. \"Literal['{REDACTED}']\" is not \"int\"; see `\"{REDACTED}\"`."
        )
    );
    assert_eq!(done.withheld, 3);
}

/// A file the row is about could not be judged: every server-written leaf
/// withholds the literals no judged file holds.
#[test]
fn an_unjudged_file_withholds_the_literals_of_names_and_details() {
    let row = || {
        json!({"matches": [{
            "symbolName": "cfg",
            "detail": format!("const cfg: {{ password: \"{PASSWORD}\"; from: \"./cfg\" }}"),
            "path": "big.ts",
        }]})
    };
    let mut judged_all = row();
    assert_eq!(
        scrub(&mut judged_all, &[judged("use.ts", USE)], false),
        Scrubbed::default()
    );
    assert_eq!(judged_all, row(), "a judged row's names stay as written");
    let mut unjudged = row();
    let done = scrub(&mut unjudged, &[judged("use.ts", USE)], true);
    assert_eq!(
        unjudged["matches"][0]["detail"],
        format!("const cfg: {{ password: \"{REDACTED}\"; from: \"./cfg\" }}")
    );
    assert_eq!(done.withheld, 1);
}

/// Text the runtime wrote is not server text: source cut by position, file
/// names, discriminators and calls stay byte for byte.
#[test]
fn runtime_written_fields_and_readable_text_stay_as_they_are() {
    let anchor = judged("cfg.ts", CFG);
    let written = json!({
        "operation": "hover",
        "status": "ok",
        "path": format!("/w/{PASSWORD}/cfg.ts"),
        "matches": [{"value": format!("1\tpassword: \"{PASSWORD}\""), "uri": format!("file:///w/{PASSWORD}.ts")}],
        "next": {"nextPage": {"input": {"queries": [{"symbolName": PASSWORD}]}}},
        "related": [{"input": {"queries": [{"matchString": PASSWORD}]}}],
        "pagination": {"note": PASSWORD},
        "hover": {"contents": "fn f<'a>(x: &'a str) -> &'a str // user: \"svc\", don't"},
    });
    let mut row = written.clone();
    assert_eq!(scrub(&mut row, &[anchor], false), Scrubbed::default());
    assert_eq!(row, written);
}

#[test]
fn quoted_spans_are_the_literals_of_one_line() {
    let spans = |text: &str, ticks: bool| {
        quoted_spans(text, ticks)
            .into_iter()
            .map(|span| text[span].to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(spans(r#"a "b\"c" 'd' `e`"#, false), [r#"b\"c"#, "d"]);
    assert_eq!(spans(r#"a "b" `e "f"`"#, true), ["b", r#"e "f""#]);
    assert_eq!(spans("\"open\nclose\" 'x", false), Vec::<String>::new());
    assert_eq!(spans("don't &'a str, it's", false), Vec::<String>::new());
}

#[test]
fn a_row_names_its_files_outside_its_calls() {
    let mut files = Vec::new();
    named_files(
        &json_fixture(
            r#"{
                "path": "a.ts",
                "matches": [{"uri": "file:///w/b.ts"}, {"location": {"path": "c.ts"}}],
                "next": {"read": {"input": {"queries": [{"path": "d.ts"}]}}},
                "related": [{"input": {"path": "e.ts"}}],
                "workspaceRoot": "/w"
            }"#,
        ),
        None,
        &mut files,
    );
    assert_eq!(files, ["a.ts", "/w/b.ts", "c.ts"]);
}

#[test]
fn a_markdown_escape_is_undone_once_and_only_before_punctuation() {
    assert_eq!(markdown_unescaped(r"secret\_key"), "secret_key");
    assert_eq!(markdown_unescaped(r"a\\_b"), r"a\_b");
    assert_eq!(markdown_unescaped(r"C:\path\n\"), r"C:\path\n\");
}

/// clangd escapes `_` before a non-ASCII letter in a hover. A hover of a
/// line a read serves whole serves it whole too, and a real assignment in
/// the same text is redacted with its key shown.
#[test]
fn a_hover_is_judged_with_its_markdown_escapes_undone() {
    let security = ContentSecurity::new();
    let source = "/** x = c ? \"CLIENT_\u{17f}ECRETKEY\" : \"Fkxh1t2IBDv6\" */\nint f(void);\n";
    let anchor = judged("a.h", source);
    let mut row = hover(
        "```cpp\n/** x = c ? \"CLIENT\\_\u{17f}ECRETKEY\" : \"Fkxh1t2IBDv6\" */\nint f(void)\n```",
    );
    scrub(&mut row, std::slice::from_ref(&anchor), false);
    let escaped = row["hover"]["contents"]["value"].as_str().expect("hover");
    assert!(
        security.sanitize_leaf(escaped).is_some(),
        "the gate reads the escaped text otherwise: {escaped}"
    );
    judge_unescaped(&mut row, &security);
    let shown = row["hover"]["contents"]["value"].as_str().expect("hover");
    assert!(shown.contains("\"Fkxh1t2IBDv6\""), "{shown}");
    assert!(security.sanitize_leaf(shown).is_none(), "{shown}");

    let source = "/** secret_Key = \"Hq7vT2mZp9XwLk3R\" */\nint g(void);\n";
    let anchor = judged("b.h", source);
    let mut row = hover("/** secret\\_Key = \"Hq7vT2mZp9XwLk3R\" */\nint g(void)");
    scrub(&mut row, std::slice::from_ref(&anchor), false);
    judge_unescaped(&mut row, &security);
    let shown = row["hover"]["contents"]["value"].as_str().expect("hover");
    assert!(!shown.contains("Hq7vT2mZp9XwLk3R"), "{shown}");
    assert!(shown.contains("Key = "), "{shown}");
    assert!(shown.contains("[REDACTED"), "{shown}");

    // A server text no judged file holds, escaped: the unescaped judgment
    // still hides the value.
    let mut row = hover("/** secret\\_Key = \"Zq8wN5vLm2XkTr7P\" */");
    let hid = judge_unescaped(&mut row, &security);
    let shown = row["hover"]["contents"]["value"].as_str().expect("hover");
    assert!(hid, "{shown}");
    assert!(!shown.contains("Zq8wN5vLm2XkTr7P"), "{shown}");
}
