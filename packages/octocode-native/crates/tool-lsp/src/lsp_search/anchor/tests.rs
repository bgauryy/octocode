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
    assert!(
        anchor.warning.is_none(),
        "the line is in the file: {receipt}"
    );
}

/// A line past the end of the file names no line of it: the row warns,
/// stating the file's length, and the receipt names the spelling nearest
/// the end beside the one chosen.
#[test]
fn a_line_past_the_end_warns_and_names_the_spelling_nearest_the_end() {
    let source = "fn check() {}\nfn a() { check(); }\nfn b() { check(); }\n";
    let length = source.lines().count();
    let asked = u32::try_from(length).expect("lines") + 1_000;
    let anchor = resolve_anchor(
        &line_query("check", asked),
        "/w/a.rs",
        "file:///w/a.rs",
        Some(source),
    )
    .expect("anchor");
    let receipt = anchor.resolved_symbol.expect("receipt");
    let found = receipt["foundAtLine"].as_u64().expect("found");
    let last = u32::try_from(length).expect("lines");
    let lines = alternative_lines(&receipt);
    assert!(
        u64::from(last) == found || lines.contains(&last),
        "the spelling nearest the end is chosen or named: {receipt}"
    );
    let warning = anchor.warning.expect("a warning");
    assert!(warning.contains(&asked.to_string()), "{warning}");
    assert!(warning.contains(&length.to_string()), "{warning}");
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
