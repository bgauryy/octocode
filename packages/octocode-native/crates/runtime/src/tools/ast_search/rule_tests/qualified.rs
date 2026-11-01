//! A `::` path pattern matches the path as written: the result counts the
//! longer spellings it left out, once, and leads to the rule listing both.

use super::*;

const CODE: &str = "structural.pattern.qualified";

fn qualified_notes(out: &serde_json::Value) -> Vec<&serde_json::Value> {
    out["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|diagnostic| diagnostic["code"] == CODE)
        .collect()
}

/// The one broadenSearch lead's row, if the result offers it.
fn broaden_row(out: &serde_json::Value) -> Option<serde_json::Value> {
    checks::find_related_call(out, "astSearch", |input| {
        input["queries"][0]["rule"].get("any").is_some()
    })
    .map(|call| call["input"]["queries"][0].clone())
}

fn matched(
    root: &std::path::Path,
    path: &std::path::Path,
    rule: serde_json::Value,
) -> serde_json::Value {
    run(root, json!({"operation":"match","path":path,"rule":rule})).expect("match")
}

/// Rows stay as written; one info diagnostic counts the longer spellings
/// (with the first one's place) and the lead lists both spellings.
#[test]
fn a_qualified_path_counts_its_longer_spellings_once() {
    let root = Fixture::new();
    let dir = root.0.join("src");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(
        dir.join("a.rs"),
        "fn f() {\n    let a = Mutex::new(None);\n    let b = Mutex::new(None);\n    let c = std::sync::Mutex::new(None);\n}\n",
    )
    .expect("a");
    std::fs::write(
        dir.join("b.rs"),
        "fn g() {\n    let d = sync::Mutex::new(1);\n}\n",
    )
    .expect("b");
    std::fs::write(dir.join("c.rs"), "fn h() {}\n").expect("c");
    let out = matched(&root.0, &dir, json!({"pattern":"Mutex::new($X)"}));
    assert_eq!(out["matchCount"], 2, "{out}");
    let notes = qualified_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    assert_eq!(notes[0]["severity"], "info", "{out}");
    let message = notes[0]["message"].as_str().expect("message");
    assert!(
        message.contains("2 more match(es) in 2 file(s)") && message.contains("`Mutex`"),
        "{message}"
    );
    assert!(message.contains("a.rs:4"), "{message}");
    let lead = broaden_row(&out).unwrap_or_else(|| panic!("broadenSearch: {out}"));
    assert_eq!(
        lead["rule"],
        json!({"any":[{"pattern":"Mutex::new($X)"},{"pattern":"$QUALIFIER::Mutex::new($X)"}]}),
        "{lead}"
    );
    crate::contracts::validate_query("astSearch", lead.clone()).expect("a valid lead row");
    let listed = run(&root.0, lead).expect("lead runs");
    assert_eq!(listed["matchCount"], 4, "{listed}");
    assert!(qualified_notes(&listed).is_empty(), "{listed}");
}

/// No longer spelling in scope: no note and no lead (a note is a fact).
#[test]
fn a_qualified_path_without_longer_spellings_adds_nothing() {
    let root = Fixture::new();
    std::fs::write(
        root.0.join("a.rs"),
        "fn f() {\n    let a = Mutex::new(None);\n}\n",
    )
    .expect("a");
    for pattern in [
        "Mutex::new($X)",
        "$P::Mutex::new($X)",
        "std::sync::Mutex::new($X)",
    ] {
        let out = matched(&root.0, &root.0, json!({"pattern": pattern}));
        assert!(qualified_notes(&out).is_empty(), "{pattern}: {out}");
        assert!(broaden_row(&out).is_none(), "{pattern}: {out}");
    }
}

/// The note belongs to the first page: a match-page continuation repeats
/// neither the note nor its lead.
#[test]
fn a_continuation_page_does_not_repeat_the_note() {
    let root = Fixture::new();
    let body = (0..40)
        .map(|n| format!("    let a{n} = Mutex::new({n});\n"))
        .collect::<String>();
    std::fs::write(
        root.0.join("a.rs"),
        format!("fn f() {{\n{body}    let q = std::sync::Mutex::new(0);\n}}\n"),
    )
    .expect("a");
    let window = 2_000;
    let first = run_window(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"Mutex::new($X)"}}),
        window,
    )
    .expect("first page");
    assert_eq!(qualified_notes(&first).len(), 1, "{first}");
    let next = first["next"]["nextMatchPage"]["input"]["queries"][0].clone();
    assert!(next.is_object(), "a second match page: {first}");
    let second = run_window(&root.0, next, window).expect("second page");
    assert!(qualified_notes(&second).is_empty(), "{second}");
    assert!(broaden_row(&second).is_none(), "{second}");
}

/// An empty file scope still says where the longer spelling is, beside the
/// syntax-tree lead.
#[test]
fn an_empty_file_scope_names_the_longer_spelling() {
    let root = Fixture::new();
    let file = root.0.join("spawn.rs");
    std::fs::write(
        &file,
        "fn f() {\n    let m = std::sync::Mutex::new(None);\n}\n",
    )
    .expect("file");
    let out = matched(&root.0, &file, json!({"pattern":"Mutex::new($X)"}));
    assert_eq!(out["status"], "empty", "{out}");
    let notes = qualified_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    assert!(
        notes[0]["message"].as_str().is_some_and(
            |m| m.contains("1 more match(es) in 1 file(s)") && m.contains("spawn.rs:2")
        ),
        "{out}"
    );
    assert!(broaden_row(&out).is_some(), "{out}");
    checks::related_call(&out, "astSearch", |input| {
        input["queries"][0]["operation"] == "syntaxTree"
    });
    assert!(
        out["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|d| d["code"] != "structural.pattern.exact"),
        "{out}"
    );
}

/// The lead keeps every other matcher the caller set beside the pattern.
#[test]
fn the_lead_keeps_the_callers_other_matchers() {
    let root = Fixture::new();
    std::fs::write(
        root.0.join("a.rs"),
        "fn f() {\n    let a = Mutex::new(1);\n    let b = std::sync::Mutex::new(2);\n}\nstatic M: u8 = sync::Mutex::new(3);\n",
    )
    .expect("a");
    let inside = json!({"kind":"function_item","stopBy":"end"});
    let out = matched(
        &root.0,
        &root.0,
        json!({"pattern":"Mutex::new($X)","inside":inside}),
    );
    assert_eq!(out["matchCount"], 1, "{out}");
    let notes = qualified_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    assert!(
        notes[0]["message"]
            .as_str()
            .is_some_and(|m| m.contains("1 more match(es)")),
        "{out}"
    );
    let lead = broaden_row(&out).unwrap_or_else(|| panic!("broadenSearch: {out}"));
    assert_eq!(lead["rule"]["inside"], inside, "{lead}");
    assert!(lead["rule"].get("pattern").is_none(), "{lead}");
    let listed = run(&root.0, lead).expect("lead runs");
    assert_eq!(listed["matchCount"], 2, "{listed}");
}

/// A scan its file cap cut never claims a count.
#[test]
fn a_truncated_scan_claims_no_count() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs"] {
        std::fs::write(
            root.0.join(name),
            "fn f() {\n    let a = Mutex::new(1);\n    let b = std::sync::Mutex::new(2);\n}\n",
        )
        .expect("source");
    }
    let _window = crate::tools::ast_search::scan_window::test_window(1);
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":"Mutex::new($X)"}}),
    )
    .expect("capped");
    assert!(qualified_notes(&out).is_empty(), "{out}");
    assert!(broaden_row(&out).is_none(), "{out}");
}

/// `.` member paths are not namespace qualifications: no note there, and a
/// Java call is not read as a declaration that differs in a modifier.
#[test]
fn member_paths_get_no_qualified_note() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.js"), "os.path.join(a, b);\n").expect("js");
    let js = matched(
        &root.0,
        &root.0.join("a.js"),
        json!({"pattern":"path.join($$$A)"}),
    );
    assert!(qualified_notes(&js).is_empty(), "{js}");
    let java = root.0.join("java");
    std::fs::create_dir_all(&java).expect("dir");
    std::fs::write(
        java.join("B.java"),
        "class B { void f() { java.util.Collections.sort(y); } }\n",
    )
    .expect("java");
    let out = matched(&root.0, &java, json!({"pattern":"Collections.sort($X)"}));
    assert_eq!(out["matchCount"], 0, "{out}");
    assert!(qualified_notes(&out).is_empty(), "{out}");
    assert!(
        out["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|d| d["code"] != "structural.pattern.exact"),
        "{out}"
    );
    assert!(broaden_row(&out).is_none(), "{out}");
}
