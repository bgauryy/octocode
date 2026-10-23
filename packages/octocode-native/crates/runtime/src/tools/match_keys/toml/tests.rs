use super::line_key_paths;

const CARGO: &str = r#"[package]
name = "demo"
version = "0.1.0"

[workspace]
members = [
"crates/a", # first
"crates/b",
]

[dependencies]
serde = { version = "1", features = ["derive"] }
tokio.workspace = true

[target.'cfg(unix)'.dependencies]
libc = "0.2"

[[bin]]
name = "a"

[[bin]]
name = "b"
description = """
multi [line] = text
"""
path = "src/b.rs"
"#;

fn keys(text: &str, lines: &[usize]) -> Vec<(usize, String)> {
    line_key_paths(text, lines)
}

fn owned(rows: &[(usize, &str)]) -> Vec<(usize, String)> {
    rows.iter()
        .map(|(line, path)| (*line, (*path).to_owned()))
        .collect()
}

/// The same entry reads differently under each table.
#[test]
fn hits_name_their_table_and_key() {
    assert_eq!(
        keys(CARGO, &[1, 2, 12, 13, 15, 16]),
        owned(&[
            (1, "package"),
            (2, "package.name"),
            (12, "dependencies.serde"),
            (13, "dependencies.tokio.workspace"),
            (15, "target.cfg(unix).dependencies"),
            (16, "target.cfg(unix).dependencies.libc"),
        ])
    );
}

/// A multi-line array names each element and, on its own lines, the
/// array; brackets and `=` inside strings and comments are text.
#[test]
fn multi_line_values_name_their_elements() {
    assert_eq!(
        keys(CARGO, &[6, 7, 8, 9, 24, 25, 26]),
        owned(&[
            (6, "workspace.members"),
            (7, "workspace.members[0]"),
            (8, "workspace.members[1]"),
            (9, "workspace.members"),
            (24, "bin[1].description"),
            (25, "bin[1].description"),
            (26, "bin[1].path"),
        ])
    );
}

/// Each `[[array]]` header opens the next element; arrays nested in an
/// element restart with it.
#[test]
fn array_tables_are_indexed() {
    assert_eq!(
        keys(CARGO, &[18, 19, 21, 22]),
        owned(&[
            (18, "bin[0]"),
            (19, "bin[0].name"),
            (21, "bin[1]"),
            (22, "bin[1].name"),
        ])
    );
    let lock = "[[package]]\nname = \"a\"\n[[package.dependencies]]\nname = \"x\"\n[[package]]\nname = \"b\"\n[[package.dependencies]]\nname = \"y\"\ndeps = [\n  { name = \"c\", marker = \"m\" },\n  { name = \"d\" },\n]\n";
    assert_eq!(
        keys(lock, &[4, 6, 8, 10, 11]),
        owned(&[
            (4, "package[0].dependencies[0].name"),
            (6, "package[1].name"),
            (8, "package[1].dependencies[0].name"),
            (10, "package[1].dependencies[0].deps[0]"),
            (11, "package[1].dependencies[0].deps[1]"),
        ])
    );
}

/// Quoted keys a dot or space would misread are quoted; a root key is
/// its own path, and a root comment has none.
#[test]
fn quoted_keys_and_the_root() {
    let text = "# top\ntitle = \"x\"\n[tool.\"my.tool\"]\n\"a b\" = 1\n";
    assert_eq!(
        keys(text, &[1, 2, 3, 4]),
        owned(&[
            (2, "title"),
            (3, r#"tool["my.tool"]"#),
            (4, r#"tool["my.tool"]["a b"]"#)
        ])
    );
}

/// Broken documents degrade to the nearest table, never fail.
#[test]
fn broken_toml_keeps_scanning() {
    let text = "[a]\nx = [1, 2\n[b]\ny = \"unterminated\nz = 1\n";
    let found = keys(text, &[2, 5]);
    assert_eq!(found[0], (2, "a.x".to_owned()));
    assert_eq!(found.len(), 2, "{found:?}");
}
