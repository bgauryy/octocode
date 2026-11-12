//! Rows of sources whose redaction spans lines, over generated layouts: a
//! declaration name is the source's identifier, and a node is the cut of its
//! own range from the source judged whole. All secrets are fake.
use crate::execution::cancel::NeverCancel;
use crate::security::JudgedText;
use crate::tools::test_support::{Fixture, simple_policy};
use octocode_contract_types::AstSearchQuery;
use serde_json::{Value, json};

/// Text no detector reads as a word: `len` characters of a fixed sequence
/// seeded by `seed`.
fn body(seed: u64, len: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut state = seed;
    (0..len)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let at = usize::try_from(state >> 33).unwrap_or(0) % ALPHABET.len();
            char::from(ALPHABET[at])
        })
        .collect()
}

/// One declaration: its text, its name, the values planted in it.
type Piece = (String, &'static str, Vec<String>);

/// The declarations of one language: a plain one, a key block in a string
/// that spans lines, a named value continued over lines, a one-line function
/// whose name's letter also occurs in the value it holds, and one more plain.
fn pieces(extension: &str, seed: u64) -> Vec<Piece> {
    let (b1, b2) = (body(seed, 64), body(seed + 1, 64));
    let value = body(seed + 2, 24);
    let inline = format!("{}f", body(seed + 3, 24));
    let tail = body(seed + 4, 36);
    let key = |lead: &str, kind: &str, close: &str| {
        format!(
            "{lead}-----BEGIN {kind}PRIVATE KEY-----\n{b1}\n{b2}\n-----END {kind}PRIVATE KEY-----{close}\n"
        )
    };
    let keyed = vec![b1.clone(), b2.clone()];
    match extension {
        "ts" => vec![
            ("export const label = \"plain\";\n".into(), "label", vec![]),
            (key("const pem = `", "", "`;"), "pem", keyed),
            (
                format!("const apiKey =\n  // the key\n  \"{value}\";\n"),
                "apiKey",
                vec![value],
            ),
            (
                format!("function f(token: string = \"{inline}\") {{ return token; }}\n"),
                "f",
                vec![inline],
            ),
            (
                format!("const split = \"ghp_\" + // joined below\n  \"{tail}\";\n"),
                "split",
                vec![tail],
            ),
            ("type Receipt = { id: number };\n".into(), "Receipt", vec![]),
        ],
        "py" => vec![
            ("label = \"plain\"\n".into(), "label", vec![]),
            (key("KEY = \"\"\"", "OPENSSH ", "\"\"\""), "KEY", keyed),
            (
                format!("AWS_SECRET_ACCESS_KEY = (\n    # wrapped\n    \"{value}\"\n)\n"),
                "AWS_SECRET_ACCESS_KEY",
                vec![value],
            ),
            (
                format!("def f(api_key=\"{inline}\"):\n    return api_key\n"),
                "f",
                vec![inline],
            ),
            (
                format!("token = \"ghp_\" \\\n    \"{tail}\"\n"),
                "token",
                vec![tail],
            ),
            ("count = 3\n".into(), "count", vec![]),
        ],
        _ => vec![
            ("var label = \"plain\"\n".into(), "label", vec![]),
            (key("var pemKey = `", "RSA ", "`"), "pemKey", keyed),
            (
                format!("var password = \"{value}\"\n"),
                "password",
                vec![value],
            ),
            (
                format!("func f() {{ secret := \"{inline}\"; _ = secret }}\n"),
                "f",
                vec![inline],
            ),
            ("type C struct{ Token string }\n".into(), "C", vec![]),
        ],
    }
}

fn run(
    file: &std::path::Path,
    extra: Value,
    (paths, security): &(
        crate::policy::path::PathPolicy,
        crate::security::ContentSecurity,
    ),
) -> Value {
    let mut row = json!({"path": file});
    row.as_object_mut()
        .expect("row")
        .extend(extra.as_object().cloned().unwrap_or_default());
    let query: AstSearchQuery = serde_json::from_value(row.clone()).expect("canonical row");
    super::super::execute_ast(&query, paths, security, &NeverCancel, None)
        .unwrap_or_else(|error| panic!("{row}: {error:?}"))
}

/// Every object of `value` that states `key`.
fn stating<'a>(value: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match value {
        Value::Array(items) => items.iter().for_each(|item| stating(item, key, out)),
        Value::Object(fields) => {
            if fields.contains_key(key) {
                out.push(value);
            }
            fields.values().for_each(|field| stating(field, key, out));
        }
        _ => {}
    }
}

/// The bytes of `text` (a source as a read shows it) a row's 1-based
/// `line`/`column` to `endLine`/`endColumn` name (ASCII sources: a column
/// is a byte).
fn row_bytes(source: &str, row: &Value) -> std::ops::Range<usize> {
    let number = |key: &str| row[key].as_u64().and_then(|n| usize::try_from(n).ok());
    let starts: Vec<usize> = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    let line = number("line").expect("line");
    let start = starts[line - 1] + number("column").expect("column") - 1;
    let end = starts[number("endLine").unwrap_or(line) - 1] + number("endColumn").expect("end") - 1;
    start..end
}

/// The node kinds that hold a string, and a pattern that binds a declared
/// name, by language.
const LANGUAGES: [(&str, &[&str], &str); 3] = [
    ("ts", &["string", "template_string"], "const $NAME = $VALUE"),
    ("py", &["string"], "$NAME = $VALUE"),
    (
        "go",
        &["interpreted_string_literal", "raw_string_literal"],
        "var $NAME = $VALUE",
    ),
];

/// Over every position of every declaration in its file, in each language:
/// each declaration is named by its identifier, no row keeps the place the
/// name was cut at, and each node is the read's text at its own range: its
/// lines and columns count the text a read shows (D4, bin16 retest), so
/// every line a node shows is that line of the read. No row shows a hidden
/// value or a placeholder of no kind.
#[test]
fn rows_of_multi_line_redactions_are_cut_at_their_own_place() {
    let mut layouts = 0;
    for (extension, kinds, binding) in LANGUAGES {
        let count = pieces(extension, 0).len();
        for turn in 0..count {
            let mut pieces = pieces(extension, 3 + 17 * u64::try_from(turn).unwrap_or(0));
            pieces.rotate_left(turn);
            let head = if extension == "go" {
                "package main\n\n"
            } else {
                ""
            };
            let source = head.to_owned()
                + &pieces
                    .iter()
                    .map(|(text, ..)| text.as_str())
                    .collect::<String>();
            assert!(source.is_ascii());
            let root = Fixture::new();
            let file = root.0.join(format!("m.{extension}"));
            std::fs::write(&file, &source).expect("source");
            let policy = simple_policy(&root.0);
            let canonical = file.canonicalize().expect("canonical");
            let judged = JudgedText::judge(&source, &canonical, &policy.1);
            let layout = format!("{extension} turn {turn}");
            let lines = source.lines().count();
            let read = judged.lines(1, lines).expect("the read").into_owned();
            let read_lines: Vec<&str> = read.split('\n').collect();
            let raw_lines: Vec<&str> = source.lines().collect();
            let hidden: Vec<&String> = pieces
                .iter()
                .flat_map(|(_, _, planted)| planted)
                .filter(|value| !read.contains(value.as_str()))
                .collect();
            assert!(
                hidden.len() >= 2,
                "{layout}: the key block is hidden: {read}"
            );
            let placeholder = octocode_engine::security::placeholder_regex();
            layouts += 1;

            let symbols = run(&file, json!({"operation": "symbols"}), &policy);
            let mut rows = Vec::new();
            stating(&symbols, "symbolName", &mut rows);
            let names: Vec<&str> = rows
                .iter()
                .filter_map(|row| row["symbolName"].as_str())
                .collect();
            for (_, name, _) in &pieces {
                assert!(names.contains(name), "{layout}: {name} of {names:?}");
            }
            for (row, name) in rows.iter().zip(&names) {
                let line = row["line"].as_u64().and_then(|n| usize::try_from(n).ok());
                let declared = line.and_then(|line| raw_lines.get(line - 1));
                assert!(
                    !name.contains("REDACTED")
                        && declared.is_some_and(|declared| declared.contains(name)),
                    "{layout}: {row}"
                );
                // A row states its name's column only to tell it from
                // another row of its name, kind and line.
                let twin = rows
                    .iter()
                    .filter(|other| {
                        ["symbolName", "kind", "line"]
                            .iter()
                            .all(|key| other[key] == row[key])
                    })
                    .count()
                    > 1;
                assert_eq!(row.get("column").is_some(), twin, "{layout}: {row}");
            }

            let mut queries: Vec<Value> = kinds
                .iter()
                .map(|kind| json!({"operation": "match", "rule": {"kind": kind}, "includeCaptures": true}))
                .collect();
            queries.push(
                json!({"operation": "match", "rule": {"pattern": binding}, "includeCaptures": true}),
            );
            let mut nodes = 0;
            for query in queries {
                let out = run(&file, query.clone(), &policy);
                let mut rows = Vec::new();
                stating(&out, "endColumn", &mut rows);
                for row in rows.iter().filter(|row| row.get("value").is_some()) {
                    nodes += 1;
                    let shown = row["value"].as_str().expect("value");
                    let at = row_bytes(&read, row);
                    assert_eq!(Some(shown), read.get(at), "{layout} {query}: {row}");
                    let first = row["line"].as_u64().and_then(|n| usize::try_from(n).ok());
                    let first = first.expect("line");
                    for (line, piece) in (first..).zip(shown.split('\n')) {
                        assert!(
                            read_lines[line - 1].contains(piece),
                            "{layout} {query}: line {line} shows {piece:?}, the read {:?}",
                            read_lines[line - 1]
                        );
                    }
                    for kind in placeholder.find_iter(shown) {
                        assert!(read.contains(kind.as_str()), "{layout} {query}: {row}");
                    }
                }
                let text = out.to_string();
                assert!(!text.contains("[REDACTED]"), "{layout} {query}: {text}");
                for value in &hidden {
                    assert!(!text.contains(value.as_str()), "{layout} {query}: {text}");
                }
            }
            assert!(nodes > pieces.len(), "{layout}: {nodes} nodes");
        }
    }
    assert!(layouts >= 15, "{layouts} layouts");
}

/// A declaration whose own name the pass replaced shows the read's
/// placeholder for it; the declarations beside it keep their names.
#[test]
fn a_name_the_pass_replaced_is_hidden_and_no_other() {
    let token = format!("ghp_{}", body(9, 36));
    let source = format!("const before = 1;\nconst {token} = 2;\nconst after = 3;\n");
    let root = Fixture::new();
    let file = root.0.join("n.ts");
    std::fs::write(&file, &source).expect("source");
    let policy = simple_policy(&root.0);
    let judged = JudgedText::judge(&source, &file.canonicalize().expect("canonical"), &policy.1);
    let read = judged.lines(2, 1).expect("line 2").into_owned();
    assert!(!read.contains(&token), "fixture no longer hidden: {read}");
    let symbols = run(&file, json!({"operation": "symbols"}), &policy);
    let mut rows = Vec::new();
    stating(&symbols, "symbolName", &mut rows);
    let names: Vec<&str> = rows
        .iter()
        .filter_map(|row| row["symbolName"].as_str())
        .collect();
    assert_eq!(names.len(), 3, "{symbols}");
    assert_eq!((names[0], names[2]), ("before", "after"), "{symbols}");
    assert!(
        !names[1].contains(&token[..12])
            && read.contains(names[1])
            && names[1].contains("[REDACTED-"),
        "{symbols}, the read {read}"
    );
}

/// A declaration row's name is the source's bytes at the place the page
/// names for it ([`super::NamePlaces`]), so a name on a line the pass hides
/// shows as that line's placeholder; a row the page names no place for is
/// left to what the pass replaced whole, and no row states a place.
#[test]
fn a_name_is_cut_at_the_place_its_page_names() {
    let root = Fixture::new();
    let (head, tail) = (body(7, 24), body(11, 40));
    let source = format!("-----BEGIN OPENSSH PRIVATE KEY-----\n{head}+{tail}\nlet after = 1;\n");
    let file = root
        .write("k.ts", &source)
        .canonicalize()
        .expect("canonical");
    let (paths, security) = simple_policy(&root.0);
    let rows = || {
        json!({"path": file, "symbols": [
            {"symbolName": head, "kind": "variable", "line": 2},
            {"symbolName": "after", "kind": "variable", "line": 3},
        ]})
    };
    let judge = |places: super::NamePlaces| {
        let mut data = rows();
        let sources = crate::tools::judged_rows::RowSources::new(&paths, &security);
        super::judge_rows(&mut data, &sources, places);
        data
    };
    let internal = [
        json!({"name": head, "kind": "variable", "line": 2}),
        json!({"name": "after", "kind": "variable", "line": 3}),
    ];
    let placed = judge(super::NamePlaces::of(&internal, &[1, 5]));
    let names = |data: &Value| {
        data["symbols"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| row["symbolName"].as_str().expect("name").to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(&placed),
        [crate::security::key_fragment_placeholder(), "after".into()],
        "{placed}"
    );
    for row in placed["symbols"].as_array().expect("rows") {
        let keys: Vec<&String> = row.as_object().expect("row").keys().collect();
        assert_eq!(keys, ["symbolName", "kind", "line"], "{row}");
    }
    // A place of another line is not this row's: the rows keep their order.
    let misplaced = judge(super::NamePlaces::of(&internal[1..], &[5]));
    assert_eq!(names(&misplaced)[1], "after", "{misplaced}");
}
