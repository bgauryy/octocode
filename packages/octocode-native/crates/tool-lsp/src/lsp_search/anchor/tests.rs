use super::*;
use octocode_test_support::json_fixture;

/// `config: target.config.clone()` spells `config` twice: the receipt
/// counts them and names the other by occurrence, column and qualified
/// name; a line with one occurrence says nothing more.
#[test]
fn a_repeated_name_on_the_line_names_its_alternatives() {
    let source = "fn f(target: T) -> L {\n    L { config: target.config.clone() }\n}\n";
    let query: LspSearchQuery = serde_json::from_value(json_fixture(
        r#"{
                "operation": "definition",
                "path": "/w/a.rs",
                "symbolName": "config",
                "line": 2
            }"#,
    ))
    .expect("query");
    let anchor = resolve_anchor(&query, "/w/a.rs", "file:///w/a.rs", Some(source)).expect("anchor");
    let symbol = anchor.resolved_symbol.expect("receipt");
    assert_eq!(symbol["occurrencesOnLine"], 2, "{symbol}");
    assert_eq!(
        symbol["alternatives"],
        json_fixture(
            r#"[
                    {
                        "occurrence": 1,
                        "foundAtCharacter": 24,
                        "symbolName": "target.config"
                    }
                ]"#
        ),
        "{symbol}"
    );
    let single: LspSearchQuery = serde_json::from_value(json_fixture(
        r#"{
                "operation": "definition",
                "path": "/w/a.rs",
                "symbolName": "target",
                "line": 1
            }"#,
    ))
    .expect("query");
    let anchor =
        resolve_anchor(&single, "/w/a.rs", "file:///w/a.rs", Some(source)).expect("anchor");
    assert!(
        anchor
            .resolved_symbol
            .expect("receipt")
            .get("alternatives")
            .is_none()
    );
}

/// A name that is not on its line resolves to the nearest line that
/// spells it; the nearest one on the other side of the asked line is
/// named as the alternative, however far, and a name found on its own
/// line names none.
#[test]
fn a_name_off_its_line_names_the_nearest_line_on_the_other_side() {
    let source = format!(
        "function run() {{}}\n{}obj.run();\n",
        "// filler\n".repeat(8)
    );
    let receipt = |line: u32| {
        let query: LspSearchQuery = serde_json::from_value(json!({
            "operation": "definition", "path": "/w/a.ts", "symbolName": "run", "line": line
        }))
        .expect("query");
        resolve_anchor(&query, "/w/a.ts", "file:///w/a.ts", Some(&source))
            .expect("anchor")
            .resolved_symbol
            .expect("receipt")
    };
    // Line 7 is three lines from the call on line 10 and six from the
    // declaration on line 1.
    let off = receipt(7);
    assert_eq!(off["foundAtLine"], 10, "{off}");
    assert_eq!(off["lineDeviation"], 3, "{off}");
    assert_eq!(
        off["alternatives"],
        json_fixture(
            r#"[
                    {
                        "foundAtLine": 1,
                        "foundAtCharacter": 10,
                        "lineDeviation": 6
                    }
                ]"#
        ),
        "{off}"
    );
    let other = receipt(3);
    assert_eq!(other["foundAtLine"], 1, "{other}");
    assert_eq!(
        other["alternatives"],
        json_fixture(
            r#"[
                    {
                        "foundAtLine": 10,
                        "foundAtCharacter": 5,
                        "lineDeviation": 7
                    }
                ]"#
        ),
        "{other}"
    );
    assert!(receipt(10).get("alternatives").is_none());
    assert!(receipt(1).get("alternatives").is_none());
}

/// A stale line resolves to the nearest spelling the resolver accepts;
/// every other spelling it chose among is as likely the one meant, on
/// either side of the asked line, and the receipt names each (nearest
/// first), with the nearest one on the other side however far.
#[test]
fn a_name_off_its_line_names_every_spelling_the_resolver_chose_among() {
    let source = format!(
        "fn a() {{}}\nfn check() {{}}\nfn b() {{ check(); }}\n// filler\n// asked\n{}fn c() {{ check(); }}\n",
        "// filler\n".repeat(14)
    );
    let query: LspSearchQuery = serde_json::from_value(json_fixture(
        r#"{
                "operation": "hover",
                "path": "/w/a.rs",
                "symbolName": "check",
                "line": 5
            }"#,
    ))
    .expect("query");
    let receipt = resolve_anchor(&query, "/w/a.rs", "file:///w/a.rs", Some(&source))
        .expect("anchor")
        .resolved_symbol
        .expect("receipt");
    assert_eq!(receipt["foundAtLine"], 3, "{receipt}");
    assert_eq!(receipt["lineDeviation"], 2, "{receipt}");
    assert_eq!(
        receipt["alternatives"],
        json_fixture(
            r#"[
                    {
                        "foundAtLine": 2,
                        "foundAtCharacter": 4,
                        "lineDeviation": 3
                    },
                    {
                        "foundAtLine": 20,
                        "foundAtCharacter": 10,
                        "lineDeviation": 15
                    }
                ]"#
        ),
        "{receipt}"
    );
}

/// A line too far from every spelling resolves to the file's one
/// declaration, by that rule and not by nearness: the spelling nearest the
/// asked line, on each side, is as likely the one meant, and the receipt
/// names it however far.
#[test]
fn a_line_far_from_every_spelling_names_the_nearest_one_beside_the_declaration() {
    let source = format!(
        "fn check() {{}}\nfn a() {{ check(); }}\n{}fn b() {{ check(); }}\n{}",
        "// filler\n".repeat(20),
        "// filler\n".repeat(20)
    );
    let near = 23_u32;
    let asked = near + 2 * DEFAULT_RADIUS as u32 + 1;
    let anchor = resolve_anchor(
        &line_query("check", asked),
        "/w/a.rs",
        "file:///w/a.rs",
        Some(&source),
    )
    .expect("anchor");
    let receipt = anchor.resolved_symbol.expect("receipt");
    assert_eq!(receipt["foundAtLine"], 1, "{receipt}");
    let lines = alternative_lines(&receipt);
    assert_eq!(lines.first(), Some(&near), "{receipt}");
    // The anchor moved off the asked line: the row says so, names the line
    // used and the other spellings, since the answer may be another's.
    let warning = anchor.warning.expect("a moved anchor warns");
    assert!(warning.contains(&asked.to_string()), "{warning}");
    assert!(warning.contains("line 1"), "{warning}");
    assert!(warning.contains(&near.to_string()), "{warning}");
}

/// A name found on its own line is the anchor asked for: no warning, and no
/// line stated beyond the request's. One found off its line with no other
/// spelling warns of the line used and names no alternative.
#[test]
fn only_a_moved_anchor_warns() {
    let source = format!("fn only() {{}}\n{}", "// filler\n".repeat(6));
    let anchor = |line: u32| {
        resolve_anchor(
            &line_query("only", line),
            "/w/a.rs",
            "file:///w/a.rs",
            Some(&source),
        )
        .expect("anchor")
    };
    assert!(anchor(1).warning.is_none());
    let moved = anchor(3);
    let warning = moved.warning.expect("a moved anchor warns");
    assert!(
        warning.contains("line 3") && warning.contains("line 1"),
        "{warning}"
    );
    assert!(!warning.contains("alternatives"), "{warning}");
}

/// A `line` far from every spelling resolves to the file's one declaration;
/// the other lines that spell the name may belong to another symbol of that
/// name, so the row's warning names every one of them, not only the nearest
/// on each side.
#[test]
fn a_far_line_resolving_to_the_declaration_warns_of_every_other_spelling() {
    let source = format!(
        "fn check() {{}}\n{}fn a() {{ check(); }}\n{}fn b() {{ check(); }}\n{}fn c() {{ check(); }}\n",
        "// filler\n".repeat(30),
        "// filler\n".repeat(30),
        "// filler\n".repeat(30),
    );
    let calls: Vec<u32> = source
        .lines()
        .enumerate()
        .filter(|(index, line)| *index > 0 && line.contains("check()"))
        .map(|(index, _)| u32::try_from(index).expect("line") + 1)
        .collect();
    assert_eq!(calls.len(), 3);
    let asked = 1 + 2 * DEFAULT_RADIUS as u32 + 1;
    let anchor = resolve_anchor(
        &line_query("check", asked),
        "/w/a.rs",
        "file:///w/a.rs",
        Some(&source),
    )
    .expect("anchor");
    let warning = anchor.warning.expect("a moved anchor warns");
    for line in calls {
        assert!(warning.contains(&line.to_string()), "{line}: {warning}");
    }
}

/// A line past the end of the file names no line of it: the read is
/// refused, stating the file's length and the lines that spell the name, so
/// no identity is chosen by name alone.
#[test]
fn a_line_past_the_end_is_refused_naming_the_length_and_the_spellings() {
    let source = "fn check() {}\nfn a() { check(); }\nfn b() { check(); }\n";
    let length = source.lines().count();
    let asked = u32::try_from(length).expect("lines") + 1_000;
    let refused = resolve_anchor(
        &line_query("check", asked),
        "/w/a.rs",
        "file:///w/a.rs",
        Some(source),
    )
    .expect_err("a line outside the file anchors nothing");
    assert!(refused.contains(&asked.to_string()), "{refused}");
    assert!(refused.contains(&format!("{length} lines")), "{refused}");
    for line in 1..=length {
        assert!(refused.contains(&line.to_string()), "{line}: {refused}");
    }
    // The file's last line is still a line of it.
    let last = u32::try_from(length).expect("lines");
    assert!(
        resolve_anchor(
            &line_query("check", last),
            "/w/a.rs",
            "file:///w/a.rs",
            Some(source)
        )
        .is_ok()
    );
}

fn line_query(name: &str, line: u32) -> LspSearchQuery {
    serde_json::from_value(serde_json::json!({
        "operation": "references",
        "path": "/w/a.rs",
        "symbolName": name,
        "line": line
    }))
    .expect("query")
}

fn alternative_lines(receipt: &Value) -> Vec<u32> {
    receipt["alternatives"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|alternative| alternative["foundAtLine"].as_u64())
        .filter_map(|line| u32::try_from(line).ok())
        .collect()
}

#[test]
fn a_stored_anchor_never_answers_for_edited_text() {
    let path = "/memo/anchor.ts";
    let before = "export function moved() {}\n";
    let after = "\n\nexport function moved() {}\n";
    let first = resolve_symbol(path, before, "moved", Some(1), None).expect("first");
    let again = resolve_symbol(path, before, "moved", Some(1), None).expect("again");
    assert_eq!(
        (again.found_at_line, again.position.character),
        (first.found_at_line, first.position.character)
    );
    let edited = resolve_symbol(path, after, "moved", Some(1), None).expect("edited");
    assert_eq!(edited.found_at_line, 3);
    assert_ne!(edited.position.line, first.position.line);
}

/// A Rust impl row's name (astSearch `Trait for Type`, rust-analyzer's
/// `impl Trait for Type` / `impl Type`) anchors on the implementing
/// type, the identifier an inherent impl row names.
#[test]
fn impl_block_names_anchor_on_the_self_type() {
    let source =
        "struct Wrap<T>(T);\nimpl<T: Clone> std::fmt::Debug for Wrap<T> {}\nimpl Wrap<u8> {}\n";
    let type_column = source.lines().nth(1).unwrap().find("Wrap").unwrap() as u32;
    for name in [
        "std::fmt::Debug for Wrap",
        "impl<T: Clone> std::fmt::Debug for Wrap<T>",
        "Debug for Wrap",
    ] {
        let resolved = resolve_symbol("/a.rs", source, name, Some(2), None)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            (resolved.found_at_line, resolved.position.character),
            (2, type_column),
            "{name}"
        );
    }
    let inherent =
        resolve_symbol("/a.rs", source, "impl Wrap<u8>", Some(3), None).expect("inherent impl");
    assert_eq!(
        (inherent.found_at_line, inherent.position.character),
        (3, 5)
    );
}

/// The anchored request `fields` adds to: `symbolName` in `/w/a.rs`.
fn named(fields: &str) -> LspSearchQuery {
    serde_json::from_value(json_fixture(&format!(
        r#"{{"operation": "callers", "path": "/w/a.rs", "symbolName": "run"{fields}}}"#
    )))
    .expect("query")
}

/// Without `line` the name anchors at its one declaration, and the anchor
/// (position, receipt, no warning) is the one the request sending that line
/// gets: uses of the name on other lines are not candidates.
#[test]
fn a_name_without_a_line_anchors_at_its_one_declaration() {
    let source = "fn main() {\n    run();\n}\n\nfn run() {}\n";
    let declared = source
        .lines()
        .zip(1u32..)
        .find_map(|(text, line)| text.starts_with("fn run").then_some(line))
        .expect("declaration line");
    let by_name = resolve_anchor(&named(""), "/w/a.rs", "file:///w/a.rs", Some(source));
    let by_line = resolve_anchor(
        &named(&format!(r#", "line": {declared}"#)),
        "/w/a.rs",
        "file:///w/a.rs",
        Some(source),
    );
    let by_name = by_name.expect("anchor by name");
    assert_eq!(by_name.line + 1, declared, "{by_name:?}");
    assert_eq!(by_name.warning, None);
    assert_eq!(Ok(by_name), by_line);
}

/// Several declarations of the name are several identities: the request is
/// refused with every candidate's line and kind, and each listed line
/// anchors when sent.
#[test]
fn a_name_declared_several_times_is_refused_with_each_line() {
    let source = "struct A;\nstruct B;\nimpl A {\n    fn run(&self) {}\n}\nimpl B {\n    fn run(&self) {}\n}\n";
    let lines = source
        .lines()
        .zip(1u32..)
        .filter_map(|(text, line)| text.contains("fn run").then_some(line))
        .collect::<Vec<_>>();
    assert!(
        lines.len() > 1,
        "the fixture declares the name more than once"
    );
    let refusal = resolve_anchor(&named(""), "/w/a.rs", "file:///w/a.rs", Some(source))
        .expect_err("several declarations never anchor");
    for line in &lines {
        assert!(refusal.contains(&format!("line {line} (")), "{refusal}");
        let sent = named(&format!(r#", "line": {line}"#));
        let anchor = resolve_anchor(&sent, "/w/a.rs", "file:///w/a.rs", Some(source));
        assert_eq!(anchor.expect("listed line anchors").line + 1, *line);
    }
    assert!(
        refusal.contains(&format!("{} times", lines.len())),
        "{refusal}"
    );
    // Each candidate names the type that holds it, never a path or an id.
    for holder in ["A", "B"] {
        assert!(
            refusal.contains(&format!("(method in {holder})")),
            "{refusal}"
        );
    }
    assert!(!refusal.contains("/w/a.rs"), "{refusal}");
}

/// A file that only uses the name declares no anchor for it: the refusal
/// says so and names `line` and the search that finds it, never the use.
#[test]
fn a_name_the_file_does_not_declare_is_refused() {
    let source = "fn main() {\n    run();\n}\n";
    let refusal = resolve_anchor(&named(""), "/w/a.rs", "file:///w/a.rs", Some(source))
        .expect_err("a use is no declaration");
    assert!(refusal.contains("declares no `run`"), "{refusal}");
    assert!(
        refusal.contains("line") && refusal.contains("localSearch"),
        "{refusal}"
    );
}

/// An `impl` block names its type and declares nothing: a type with impls
/// has one declaration.
#[test]
fn an_impl_block_is_no_second_declaration_of_its_type() {
    let source = "struct Runner;\n\nimpl Runner {\n    fn go(&self) {}\n}\n";
    let query: LspSearchQuery = serde_json::from_value(json_fixture(
        r#"{"operation": "references", "path": "/w/a.rs", "symbolName": "Runner"}"#,
    ))
    .expect("query");
    let anchor = resolve_anchor(&query, "/w/a.rs", "file:///w/a.rs", Some(source));
    assert_eq!(anchor.expect("one declaration").line, 0);
}

/// The request a rung reads carries the declaration's line, so every rung
/// answers the request that sends it; one with a line is left as sent.
#[test]
fn a_request_without_a_line_is_rewritten_to_the_declaration_line() {
    let directory = tempfile::tempdir().expect("directory");
    let file = directory.path().join("a.rs");
    std::fs::write(&file, "fn main() {\n    run();\n}\n\nfn run() {}\n").expect("write");
    let path = file.to_string_lossy();
    let rewritten = anchored_by_name(&named(""), &path)
        .expect("one declaration")
        .expect("rewritten");
    assert_eq!(lsp_search_query::anchor_line(&rewritten), Some(5));
    assert_eq!(
        lsp_search_query::to_row(&rewritten),
        lsp_search_query::to_row(&named(r#", "line": 5"#))
    );
    assert_eq!(anchored_by_name(&rewritten, &path), Ok(None));
    std::fs::write(&file, "fn main() {\n    run();\n}\n").expect("write");
    assert!(anchored_by_name(&named(""), &path).is_err());
}
