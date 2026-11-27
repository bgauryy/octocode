use super::*;

const PACKAGE: &str = r#"{
  "name": "ext",
  "activationEvents": [
    "onLanguage:json",
    "onCommand:ext.run"
  ],
  "contributes": {
    "commands": [
      { "command": "ext.run", "title": "onLanguage" }
    ],
    "configuration": { "ext.on": { "default": "onCommand" } }
  }
}
"#;

/// The kept lines of `scope` in `text` (a document at `name`), each line
/// once, and the kept lines it shares with members outside it.
fn kept(name: &str, text: &str, scope: &str) -> (Vec<u32>, Vec<u32>) {
    let lines = scope_lines(&steps(scope), Path::new(name), text);
    let kept = lines
        .spans
        .iter()
        .flat_map(|[start, end]| *start..=*end)
        .collect();
    (kept, lines.shared)
}

/// Each scope keeps the lines its value is on, written out by hand per
/// fixture: multi-line values, a one-line object or array inside a
/// pretty-printed document, a minified document and TOML inline values.
/// A kept line that holds members outside the scope is named shared.
#[test]
fn a_scope_keeps_the_lines_its_value_is_on() {
    const ONE_LINE: &str = r#"{
  "activationEvents": ["onLanguage:json", "onCommand:ext.run"],
  "main": "./out/ext.js"
}
"#;
    const MINIFIED: &str = r#"{"name":"ext","activationEvents":["onLanguage:json"],"main":"x"}"#;
    const CARGO: &str = r#"[package]
name = "x"

[dependencies]
serde = { version = "1", features = ["derive"] }
anyhow = "1"
"#;
    let none: Vec<u32> = Vec::new();
    for (name, text, scope, lines, shared) in [
        (
            "package.json",
            PACKAGE,
            "activationEvents",
            vec![3, 4, 5, 6],
            none.clone(),
        ),
        (
            "package.json",
            PACKAGE,
            "activationEvents[1]",
            vec![5],
            none.clone(),
        ),
        (
            "package.json",
            PACKAGE,
            "contributes.commands",
            vec![8, 9, 10],
            none.clone(),
        ),
        (
            "package.json",
            PACKAGE,
            "contributes.commands[*].title",
            vec![9],
            vec![9],
        ),
        (
            "package.json",
            PACKAGE,
            "contributes.commands[0].command",
            vec![9],
            vec![9],
        ),
        (
            "package.json",
            PACKAGE,
            "contributes.*",
            vec![8, 9, 10, 11],
            none.clone(),
        ),
        (
            "package.json",
            PACKAGE,
            r#"contributes.configuration["ext.on"]"#,
            vec![11],
            vec![11],
        ),
        (
            "package.json",
            PACKAGE,
            r#"contributes.configuration["ext.on"].default"#,
            vec![11],
            vec![11],
        ),
        (
            "package.json",
            PACKAGE,
            "missing",
            none.clone(),
            none.clone(),
        ),
        (
            "package.json",
            ONE_LINE,
            "activationEvents",
            vec![2],
            none.clone(),
        ),
        (
            "package.json",
            ONE_LINE,
            "activationEvents[*]",
            vec![2],
            vec![2],
        ),
        (
            "package.json",
            ONE_LINE,
            "activationEvents[1]",
            vec![2],
            vec![2],
        ),
        ("package.json", ONE_LINE, "main", vec![3], none.clone()),
        (
            "package.json",
            MINIFIED,
            "activationEvents",
            vec![1],
            vec![1],
        ),
        ("package.json", MINIFIED, "name", vec![1], vec![1]),
        (
            "package.json",
            MINIFIED,
            "missing",
            none.clone(),
            none.clone(),
        ),
        (
            "Cargo.toml",
            CARGO,
            "dependencies",
            vec![4, 5, 6],
            none.clone(),
        ),
        (
            "Cargo.toml",
            CARGO,
            "dependencies.serde",
            vec![5],
            none.clone(),
        ),
        (
            "Cargo.toml",
            CARGO,
            "dependencies.serde.version",
            vec![5],
            vec![5],
        ),
        (
            "Cargo.toml",
            CARGO,
            "dependencies.serde.features[0]",
            vec![5],
            vec![5],
        ),
        (
            "Cargo.toml",
            CARGO,
            "dependencies.anyhow",
            vec![6],
            none.clone(),
        ),
    ] {
        assert_eq!(kept(name, text, scope), (lines, shared), "{name} {scope}");
    }
}

/// Key paths read back into the steps they were spelled from: plain keys,
/// quoted keys holding dots, brackets and quotes, and indices.
#[test]
fn key_paths_read_back_into_their_steps() {
    for (path, expected) in [
        ("a.b", vec![Step::Key("a".into()), Step::Key("b".into())]),
        (
            r#"exports["./node"].import"#,
            vec![
                Step::Key("exports".into()),
                Step::Key("./node".into()),
                Step::Key("import".into()),
            ],
        ),
        (
            r#"a["x]\"y"][2]"#,
            vec![
                Step::Key("a".into()),
                Step::Key("x]\"y".into()),
                Step::Index("2".into()),
            ],
        ),
        (
            "[*].name",
            vec![Step::Index("*".into()), Step::Key("name".into())],
        ),
    ] {
        assert_eq!(steps(path), expected, "{path}");
    }
}

/// Only JSON and TOML files can hold a kept line.
#[test]
fn only_keyed_files_are_held() {
    let scope = line_scope("dependencies");
    for (path, keyed) in [
        ("Cargo.toml", true),
        ("package.json", true),
        ("a.rs", false),
    ] {
        assert_eq!((scope.holds)(Path::new(path)), keyed, "{path}");
    }
}
