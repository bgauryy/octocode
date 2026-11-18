use super::*;

const RUST: &str = "fn r#type() {}\nfn ordinary() {}\nfn unrelated_type() {}\nfn call() { r#type(); r#ordinary(); unrelated_type(); }\n";

fn source_matches(pages: &[Value]) -> Vec<Value> {
    pages
        .iter()
        .flat_map(|page| {
            assert_syntax(page);
            page["files"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|file| file["matches"].as_array().into_iter().flatten())
                .cloned()
        })
        .collect()
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn rust_raw_identifier_definitions_accept_canonical_and_source_spelling() {
    let (_dirs, file, execution) = fixture("lib.rs", RUST, ".rs");
    let root = file.parent().expect("root");
    for (name, line, source) in [
        ("type", 1, "fn r#type() {}"),
        ("r#type", 1, "fn r#type() {}"),
        ("ordinary", 2, "fn ordinary() {}"),
        ("r#ordinary", 2, "fn ordinary() {}"),
    ] {
        let query = json!({"operation":"definition", "path":file.to_string_lossy(),
            "symbolName":name, "line":4});
        let rows = source_matches(&walk(root, &execution, query, None).await);
        assert_eq!(rows.len(), 1, "{name}: {rows:?}");
        assert_eq!(rows[0]["line"], line, "{name}: {rows:?}");
        assert_eq!(rows[0]["column"], 4, "{name}: {rows:?}");
        assert!(
            rows[0]["content"]
                .as_str()
                .is_some_and(|text| text.contains(source)),
            "{name}: {rows:?}"
        );
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn rust_raw_identifier_references_keep_the_whole_spelling_and_roles() {
    let (_dirs, file, execution) = fixture("lib.rs", RUST, ".rs");
    let root = file.parent().expect("root");
    for (name, declaration_line, spelling) in [
        ("type", 1, "r#type"),
        ("r#type", 1, "r#type"),
        ("ordinary", 2, "r#ordinary"),
        ("r#ordinary", 2, "r#ordinary"),
    ] {
        for include in [true, false] {
            let query = json!({"operation":"references", "path":file.to_string_lossy(),
                "symbolName":name, "line":declaration_line, "includeDeclaration":include});
            let rows = source_matches(&walk(root, &execution, query, None).await);
            assert_eq!(rows.len(), if include { 2 } else { 1 }, "{name}: {rows:?}");
            let call = rows.iter().find(|row| row["line"] == 4).expect("call");
            let column = RUST
                .lines()
                .nth(3)
                .expect("call line")
                .find(spelling)
                .expect("raw use")
                + 1;
            assert_eq!(call["column"], column, "{name}: {rows:?}");
            assert_eq!(call["role"], "call", "{name}: {rows:?}");
            if include {
                let declaration = rows
                    .iter()
                    .find(|row| row["line"] == declaration_line)
                    .expect("declaration");
                assert_eq!(declaration["column"], 4, "{name}: {rows:?}");
                assert_eq!(declaration["role"], "declaration", "{name}: {rows:?}");
            }
        }
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn rust_raw_identifier_hover_and_callers_accept_both_spellings() {
    let (_dirs, file, execution) = fixture("lib.rs", RUST, ".rs");
    let root = file.parent().expect("root");
    for name in ["type", "r#type"] {
        for (operation, expected) in [
            ("hover", "fn r#type() {}"),
            ("callers", r#""symbolName":"call""#),
        ] {
            let query = json!({"operation":operation, "path":file.to_string_lossy(),
                "symbolName":name, "line":1});
            let pages = walk(root, &execution, query, None).await;
            assert_syntax(&pages[0]);
            assert!(
                pages[0].to_string().contains(expected),
                "{operation} {name}: {pages:?}"
            );
        }
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn syntax_references_exclude_comments_and_strings_and_keep_same_line_sites() {
    for (file_name, extension, source, name, spelling) in [
        (
            "lib.rs",
            ".rs",
            "fn r#type() {}\nfn call() { r#type(); let label = \"r#type\"; r#type(); } // r#type\n/* r#type */\n",
            "type",
            "r#type",
        ),
        (
            "index.ts",
            ".ts",
            "function target() {}\nfunction call() { const label = \"🌍 target\"; const pattern = /target/; target(); target(); } // target\n/* target */\n",
            "target",
            "target",
        ),
    ] {
        let (_dirs, file, execution) = fixture(file_name, source, extension);
        let root = file.parent().expect("root");
        let query = json!({"operation":"references", "path":file.to_string_lossy(),
            "symbolName":name, "line":1, "includeDeclaration":false});
        let rows = source_matches(&walk(root, &execution, query, None).await);
        assert_eq!(rows.len(), 2, "{file_name}: {rows:?}");
        let line = source.lines().nth(1).expect("call line");
        let expected = line
            .match_indices(&format!("{spelling}()"))
            .map(|(start, _)| line[..start].encode_utf16().count() + 1)
            .collect::<Vec<_>>();
        let actual = rows
            .iter()
            .map(|row| {
                assert_eq!(row["line"], 2, "{file_name}: {row}");
                assert_eq!(row["role"], "call", "{file_name}: {row}");
                row["column"].as_u64().expect("source column") as usize
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{file_name}: {rows:?}");
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn syntax_references_disclose_a_source_whose_parse_cannot_establish_occurrences() {
    let (_dirs, file, execution) = fixture(
        "index.ts",
        "function target() {}\nfunction broken( { target();\n",
        ".ts",
    );
    let root = file.parent().expect("root");
    let query = json!({"operation":"references", "path":file.to_string_lossy(),
        "symbolName":"target", "line":1});
    let pages = walk(root, &execution, query, None).await;
    assert_syntax(&pages[0]);
    let warnings = pages[0]["warnings"].to_string();
    assert!(
        warnings.contains("unread") && warnings.contains("index.ts"),
        "{pages:?}"
    );
    assert!(
        pages[0]["related"].to_string().contains("localSearch"),
        "{pages:?}"
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn rust_raw_trait_implementation_and_type_hierarchy_accept_both_spellings() {
    let source = "trait r#Trait {}\nstruct S;\nimpl Trait for S {}\ntrait TraitSuffix {}\nstruct Other;\nimpl TraitSuffix for Other {}\n";
    let (_dirs, file, execution) = fixture("lib.rs", source, ".rs");
    let root = file.parent().expect("root");
    for name in ["Trait", "r#Trait"] {
        let query = json!({"operation":"implementation", "path":file.to_string_lossy(),
            "symbolName":name, "line":1});
        let rows = source_matches(&walk(root, &execution, query, None).await);
        assert_eq!(rows.len(), 1, "{name}: {rows:?}");
        assert_eq!(rows[0]["line"], 3, "{name}: {rows:?}");
        let query = json!({"operation":"subtypes", "path":file.to_string_lossy(),
            "symbolName":name, "line":1});
        let pages = walk(root, &execution, query, None).await;
        assert_syntax(&pages[0]);
        let mut names = Vec::new();
        collect_names(&pages[0], &mut names);
        assert!(names.iter().any(|name| name == "S"), "{name}: {pages:?}");
        assert!(
            !names.iter().any(|name| name == "Other"),
            "{name}: {pages:?}"
        );
    }
    let query = json!({"operation":"supertypes", "path":file.to_string_lossy(),
        "symbolName":"S", "line":3});
    let pages = walk(root, &execution, query, None).await;
    assert_syntax(&pages[0]);
    let mut names = Vec::new();
    collect_names(&pages[0], &mut names);
    assert!(names.iter().any(|name| name == "r#Trait"), "{pages:?}");
}

fn collect_names(value: &Value, names: &mut Vec<String>) {
    match value {
        Value::Object(fields) => {
            if let Some(name) = fields.get("symbolName").and_then(Value::as_str) {
                names.push(name.to_owned());
            }
            for value in fields.values() {
                collect_names(value, names);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_names(value, names);
            }
        }
        _ => {}
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn escaped_identifier_definitions_and_references_accept_both_spellings() {
    for (file_name, extension, source, canonical, spelling) in [
        (
            "Demo.cs",
            ".cs",
            "class Demo {\n    void @class() {}\n    void Use() { @class(); }\n}\n",
            "class",
            "@class",
        ),
        (
            "Demo.scala",
            ".scala",
            "object Demo {\n  def `type`(): Unit = ()\n  def use(): Unit = { `type`() }\n}\n",
            "type",
            "`type`",
        ),
    ] {
        let (_dirs, file, execution) = fixture(file_name, source, extension);
        let root = file.parent().expect("root");
        for name in [canonical, spelling] {
            let query = json!({"operation":"definition", "path":file.to_string_lossy(),
                "symbolName":name, "line":3});
            let rows = source_matches(&walk(root, &execution, query, None).await);
            assert_eq!(rows.len(), 1, "{file_name} {name}: {rows:?}");
            assert_eq!(rows[0]["line"], 2, "{file_name} {name}: {rows:?}");
            let column = source
                .lines()
                .nth(1)
                .expect("declaration")
                .find(spelling)
                .expect("source name")
                + 1;
            assert_eq!(rows[0]["column"], column, "{file_name} {name}: {rows:?}");
            let query = json!({"operation":"references", "path":file.to_string_lossy(),
                "symbolName":name, "line":2, "includeDeclaration":false});
            let rows = source_matches(&walk(root, &execution, query, None).await);
            assert_eq!(rows.len(), 1, "{file_name} {name}: {rows:?}");
            assert_eq!(rows[0]["line"], 3, "{file_name} {name}: {rows:?}");
            assert_eq!(rows[0]["role"], "call", "{file_name} {name}: {rows:?}");
        }
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn syntax_references_keep_a_quoted_method_declaration_but_exclude_value_strings() {
    let source = "class Box { \"run\"() {} }\nconst label = \"run\"; // run\nnew Box().run();\n";
    let (_dirs, file, execution) = fixture("index.js", source, ".js");
    let root = file.parent().expect("root");
    for include in [true, false] {
        let query = json!({"operation":"references", "path":file.to_string_lossy(),
            "symbolName":"run", "line":1, "includeDeclaration":include});
        let rows = source_matches(&walk(root, &execution, query, None).await);
        assert_eq!(rows.len(), if include { 2 } else { 1 }, "{rows:?}");
        assert!(
            rows.iter()
                .any(|row| row["line"] == 3 && row["role"] == "call"),
            "{rows:?}"
        );
        if include {
            assert!(
                rows.iter()
                    .any(|row| row["line"] == 1 && row["role"] == "declaration"),
                "{rows:?}"
            );
        }
    }
}
