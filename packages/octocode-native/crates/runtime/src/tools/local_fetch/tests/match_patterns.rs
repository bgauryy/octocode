//! How `matchString` patterns match: case mode, PCRE2, Unicode offsets,
//! literal lists, and the key paths structured hits name.

use super::*;

/// An omitted caseMode is smart case, like localSearch: a
/// lowercase pattern ignores case, an uppercase letter makes it exact.
#[test]
fn match_case_defaults_to_smart_case() {
    use octocode_contract_types::ReadCaseMode;
    let t = Fixture::new();
    let p = t.0.join("a.py");
    fs::write(&p, "MERGE_SETTINGS = 1\nmerge_settings()\nMerge_Settings\n").expect("fixture");
    let paths = Paths(t.0.clone());
    for (pattern, mode, lines) in [
        ("merge_settings", None, serde_json::json!([1, 2, 3])),
        ("MERGE_SETTINGS", None, serde_json::json!([1])),
        (
            "MERGE_SETTINGS",
            Some(ReadCaseMode::Insensitive),
            serde_json::json!([1, 2, 3]),
        ),
        (
            "merge_settings",
            Some(ReadCaseMode::Sensitive),
            serde_json::json!([2]),
        ),
    ] {
        let mut req = q(&p);
        req.match_string = Some(MatchString::String(pattern.parse().expect("match string")));
        req.case_mode = mode;
        req.context_lines = Some(0);
        let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
        assert_eq!(wire["matchedLines"], lines, "{pattern} {mode:?}: {wire}");
    }
}

/// `regex:"pcre2"` (localSearch's engine for lookaround/backreferences)
/// matches in original byte coordinates, and a multiline match keeps every
/// source line it touches.
#[test]
fn pcre2_matches_lookaround_backreferences_and_multiline_spans() {
    use octocode_contract_types::ReadRegex;
    let t = Fixture::new();
    let p = t.0.join("a.rs");
    fs::write(&p, "before\nfn alpha() {\n  beta(); beta();\n}\nafter\n").expect("fixture");
    let paths = Paths(t.0.clone());
    let read = |pattern: &str| {
        let mut req = q(&p);
        req.match_string = Some(MatchString::String(pattern.parse().expect("match string")));
        req.regex = Some(ReadRegex::Pcre2);
        req.context_lines = Some(0);
        serde_json::to_value(fetch(&req, &paths)).expect("serializable")
    };
    let lookbehind = read("(?<=fn )alpha");
    assert_eq!(
        lookbehind["matchedLines"],
        serde_json::json!([2]),
        "{lookbehind}"
    );
    let backreference = read(r"(beta\(\);) \1");
    assert_eq!(
        backreference["matchedLines"],
        serde_json::json!([3]),
        "{backreference}"
    );
    for (engine, pattern) in [
        (ReadRegex::Pcre2, r"alpha\(\) \{\n\s+beta"),
        (ReadRegex::Rust, r"alpha\(\) \{\n\s+beta"),
    ] {
        let mut req = q(&p);
        req.match_string = Some(MatchString::String(pattern.parse().expect("match string")));
        req.regex = Some(engine);
        req.context_lines = Some(0);
        let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
        assert_eq!(
            wire["content"], "fn alpha() {\n  beta(); beta();\n",
            "{engine}: {wire}"
        );
        assert_eq!(
            wire["matchedLines"],
            serde_json::json!([2, 3]),
            "{engine}: {wire}"
        );
        assert_eq!(
            wire["sourceLineRanges"],
            serde_json::json!([{"line": 2, "endLine": 3}]),
            "{engine}: {wire}"
        );
    }
    // Uppercase in a regex is smart case too.
    assert_eq!(read("ALPHA")["content"], "", "no line has uppercase ALPHA");
    assert_eq!(read("alph[a]")["matchedLines"], serde_json::json!([2]));
    let invalid = read("(");
    assert_eq!(invalid["errorCode"], "invalidPattern", "{invalid}");
}

#[test]
fn byte_windows_keep_original_offsets_when_unicode_lowercase_expands() {
    for (source, pattern, expected) in [
        ("İneedle tail\n", "needle", "needle"),
        ("İNEEDLE\n", "needle", "NEEDLE"),
        ("prefix İ tail\n", "İ", "İ"),
        ("İİneedle", "needle", "needle"),
    ] {
        let sensitive = pattern.chars().any(char::is_uppercase);
        let spans =
            crate::tools::source_view::extraction::literal_ranges(source, pattern, sensitive);
        let (content, _) = crate::tools::source_view::extraction::byte_windows(source, spans, 0);
        // A window that starts or ends mid-line says so with an omission
        // marker; the matched bytes between them are the original span.
        let span = content
            .lines()
            .filter(|line| !line.starts_with("... ["))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(span, expected, "{source:?}: {content:?}");
        let cut_before = source.find(expected).is_some_and(|at| at > 0);
        assert_eq!(
            content.starts_with("... ["),
            cut_before,
            "{source:?}: {content:?}"
        );
    }
}

#[test]
fn a_match_list_is_a_grep_map_of_every_literal() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(
        &p,
        numbered(12)
            .replace("l4\n", "def a|b\n")
            .replace("l9\n", "x.y\n"),
    )
    .expect("fixture");
    let paths = Paths(t.0.clone());
    // Literal entries are literal: `|` and `.` are not regex syntax.
    let req = qj(
        &p,
        serde_json::json!({"matchString": ["a|b", "x.y", "absent"], "contextLines": 0}),
    );
    let r = fetch(&req, &paths);
    assert_eq!(
        r.content.as_deref(),
        Some("def a|b\n... [lines 5-8 not requested] ...\nx.y\n")
    );
    assert_eq!(r.matched_lines, vec![4, 9]);
    let one = qj(
        &p,
        serde_json::json!({"matchString": "x.y", "contextLines": 0}),
    );
    assert_eq!(fetch(&one, &paths).matched_lines, vec![9]);
}

/// A matchString read of a JSON file names each hit line's section: a
/// window that hides the section header still says `devDependencies.vite`.
/// Other files carry no `matchKeys`.
#[test]
fn json_match_reads_name_each_hit_line_key_path() {
    let t = Fixture::new();
    let mut text = String::from("{\n  \"dependencies\": {\n");
    for n in 0..40 {
        text.push_str(&format!("    \"dep{n}\": \"1\",\n"));
    }
    text.push_str("    \"vite\": \"^7\"\n  },\n  \"devDependencies\": {\n");
    for n in 0..40 {
        text.push_str(&format!("    \"dev{n}\": \"1\",\n"));
    }
    text.push_str("    \"vite\": \"^7\"\n  }\n}\n");
    let json = t.0.join("package.json");
    fs::write(&json, &text).expect("fixture");
    let paths = Paths(t.0.clone());
    let req = qj(
        &json,
        serde_json::json!({"matchString": "\"vite\"", "contextLines": 0}),
    );
    let wire = serde_json::to_value(fetch(&req, &paths)).expect("serialize");
    assert_eq!(
        wire["matchKeys"],
        serde_json::json!({"43": "dependencies.vite", "86": "devDependencies.vite"}),
        "{wire}"
    );
    let plain = t.0.join("notes.txt");
    fs::write(&plain, &text).expect("fixture");
    let wire = serde_json::to_value(fetch(
        &qj(&plain, serde_json::json!({"matchString": "\"vite\""})),
        &paths,
    ))
    .expect("serialize");
    assert!(wire.get("matchKeys").is_none(), "{wire}");
}

/// A matchString read of a TOML file names each hit's table and key, with
/// the same `matchKeys` map (`[target.'cfg(…)'.dependencies]`).
#[test]
fn toml_match_reads_name_each_hit_line_key_path() {
    let t = Fixture::new();
    let toml = t.0.join("Cargo.toml");
    fs::write(
        &toml,
        "[package]\nname = \"demo\"\n\n[dependencies]\nbacktrace = \"0.3\"\n\n[target.'cfg(unix)'.dev-dependencies]\nbacktrace = \"0.3\"\n",
    )
    .expect("fixture");
    let paths = Paths(t.0.clone());
    let wire = serde_json::to_value(fetch(
        &qj(
            &toml,
            serde_json::json!({"matchString": "backtrace", "contextLines": 0}),
        ),
        &paths,
    ))
    .expect("serialize");
    assert_eq!(
        wire["matchKeys"],
        serde_json::json!({"5": "dependencies.backtrace", "8": "target.cfg(unix).dev-dependencies.backtrace"}),
        "{wire}"
    );
}
