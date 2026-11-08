//! Symbols outlines: declaration rows, comment lines, over-long rows, and no size cap.

use super::*;

/// A row past the outline's line width lists the heads of the declarations
/// it holds, each to the end of its name, so a minified or deeply nested
/// line keeps every declaration and the rows around it stay as written.
#[test]
fn an_over_long_row_lists_the_heads_of_its_declarations() {
    let mut source = String::new();
    for i in 0..200 {
        source.push_str(&format!(
            "pub fn run{i}(value: usize) -> usize {{\n    let doubled = value * 2;\n    doubled + {i}\n}}\n\n"
        ));
    }
    let fields = (0..120)
        .map(|i| format!("pub field_{i}: usize"))
        .collect::<Vec<_>>()
        .join(", ");
    let long = format!("pub struct Generated {{ {fields} }} pub fn after() {{}}");
    assert!(long.chars().count() > super::super::outline::OUTLINE_MAX_LINE_CHARS);
    source.push_str(&long);
    source.push('\n');
    let outline = extract_signatures_inner(&source, "lib.rs").expect("outline");
    assert!(
        outline.contains("pub fn run199(value: usize) -> usize"),
        "{outline}"
    );
    let row = outline
        .lines()
        .find(|row| row.contains("Generated"))
        .unwrap_or_else(|| panic!("a row for the long line in {outline}"));
    let heads = (0..120)
        .map(|i| format!(" pub field_{i} …"))
        .collect::<String>();
    assert!(
        row.ends_with(&format!("\tpub struct Generated …{heads} pub fn after …")),
        "{row}"
    );

    // An unclosed expression nested past any parser's stack, on one line.
    let deep = format!(
        "function marker() {{ return 1; }}\nconst x = {};\n",
        "(".repeat(50_000)
    );
    let outline = extract_signatures_inner(&deep, "deep.js").expect("outline");
    assert!(
        outline.contains("1\tfunction marker() { return 1; }"),
        "{outline}"
    );
    assert!(outline.len() < 200, "{outline}");
}

/// A namespace or module body holds statements, as a file does: its outline
/// is its declarations. A type's body lists its member rows; a closing row,
/// alone or before a comment, is never one.
#[test]
fn only_a_type_body_lists_its_member_rows() {
    let source = "namespace ns {\nstatic_assert(sizeof(int) == 4);\nclass A {\n  int field;\n  void m() {\n    run();\n  }\n};  // class A\n}  // namespace ns\n";
    let outline = extract_signatures_inner(source, "a.cpp").expect("outline");
    for row in ["\tclass A {", "\t  int field;", "\t  void m() {"] {
        assert!(outline.contains(row), "{row:?} in {outline}");
    }
    for text in ["static_assert", "run();", "// class A", "// namespace ns"] {
        assert!(!outline.contains(text), "{text:?} in {outline}");
    }
}

/// Comment lines follow the file's own grammar: `#` comments Python,
/// not Rust; `--` comments neither.
#[test]
fn comment_lines_follow_the_files_grammar() {
    assert_eq!(is_comment_line("a.py", "# note"), Some(true));
    assert_eq!(is_comment_line("a.rs", "# note"), Some(false));
    assert_eq!(is_comment_line("a.rs", "/// doc"), Some(true));
    assert_eq!(is_comment_line("a.rs", "-- x"), Some(false));
    assert_eq!(is_comment_line("notes.txt", "// x"), None);
}

/// Each missing outline names its own reason.
#[test]
fn a_missing_outline_names_its_reason() {
    assert_eq!(
        extract_signatures_inner("# Title\n\nText\n", "README.md"),
        Err(OutlineUnavailable::UnsupportedLanguage)
    );
    assert_eq!(
        extract_signatures_inner("fn a() {}\n", "tiny.rs"),
        Err(OutlineUnavailable::NotSmaller)
    );
    assert_eq!(
        extract_signatures_inner("// only a comment\n", "empty.rs"),
        Err(OutlineUnavailable::NoDeclarations)
    );
}

/// No size refuses an outline: a source as large as a whole read admits
/// outlines in full; the parse's linear work bound is what stops a parse.
#[test]
fn a_source_as_large_as_a_whole_read_outlines_in_full() {
    let body = "fn f() {\n    let x = 1;\n    x\n}\n";
    let count = crate::security::MAX_CONTENT_SIZE / body.len();
    let source = (0..count)
        .map(|i| body.replace("fn f", &format!("fn f{i}")))
        .collect::<String>();
    let outline = extract_signatures_inner(&source, "big.rs").expect("outline");
    assert_eq!(outline.lines().count(), count);
    assert!(
        outline.ends_with(&format!("\tfn f{}() {{", count - 1)),
        "{}",
        &outline[outline.len() - 80..]
    );
}

/// Gutters are unpadded so the content-view minifier (which trims
/// leading whitespace on some lines) cannot leave them uneven.
#[test]
fn skeleton_gutters_are_uniform_after_minification() {
    let mut source = String::new();
    for i in 0..40 {
        source.push_str(&format!(
            "impl S{i} {{\n    pub fn run(&self) -> usize {{\n        let x = {i};\n        x + 1\n    }}\n}}\n\n"
        ));
    }
    let mut python = String::new();
    for i in 0..40 {
        python.push_str(&format!(
            "class C{i}:\n    def run(self):\n        x = {i}\n        y = x + 1\n        return y\n\n"
        ));
    }
    let mut views = Vec::new();
    for (path, text) in [("lib.rs", &source), ("mod.py", &python)] {
        views.push(extract(text, path).expect("skeleton"));
    }
    for view in &views {
        for line in view.lines() {
            assert!(
                line.split_once('\t')
                    .is_some_and(|(n, _)| n.parse::<usize>().is_ok()),
                "{line:?} in\n{view}"
            );
        }
    }
}

// ── tree-sitter languages ─────────────────────────────────────────────────
/// Each tree-sitter skeleton keeps declarations and drops bodies.
#[test]
fn tree_sitter_skeletons_keep_signatures_and_drop_bodies() {
    let cases: [(&str, &str, &[&str], &[&str]); 6] = [
        (
            "calc.ts",
            "\nexport function add(a: number, b: number): number {\n  return a + b;\n}\n\nexport class Calc {\n  value: number = 0;\n  multiply(x: number): number {\n    return this.value * x;\n  }\n}\n",
            // function, class, field, method signature
            &["add", "Calc", "value", "multiply"],
            &["return a + b", "this.value * x"],
        ),
        (
            "foo.py",
            "\nimport os\n\nclass Foo:\n    name: str\n\n    def bar(self, x: int) -> str:\n        return str(x)\n\ndef top_level():\n    pass\n",
            &["import os", "class Foo", "def bar", "def top_level"],
            &["return str", "pass"],
        ),
        (
            "geo.rs",
            "\npub fn greet(name: &str) -> String {\n    format!(\"Hello, {}\", name)\n}\n\npub struct Point { x: f64, y: f64 }\n\nimpl Point {\n    pub fn distance(&self, other: &Point) -> f64 {\n        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()\n    }\n}\n",
            &["greet"],
            &["format!"],
        ),
        (
            "main.go",
            "\npackage main\n\nimport \"fmt\"\n\nfunc Add(a, b int) int {\n    return a + b\n}\n\ntype Server struct {\n    Port int\n}\n\nfunc (s *Server) Start() error {\n    fmt.Println(\"starting\")\n    return nil\n}\n",
            &["Add"],
            &["Println"],
        ),
        (
            "Calculator.java",
            "\npublic class Calculator {\n    private int value;\n\n    public Calculator(int initial) {\n        this.value = initial;\n    }\n\n    public int add(int x) {\n        return value + x;\n    }\n}\n",
            &["Calculator", "add"],
            &["return value"],
        ),
        (
            "math.c",
            "\n#include <stdio.h>\n\nint add(int a, int b) {\n    return a + b;\n}\n\nvoid greet(const char *name) {\n    printf(\"Hello, %s\\n\", name);\n}\n",
            &["add"],
            &["printf"],
        ),
    ];
    for (file, src, kept, dropped) in cases {
        let s = extract(src, file).unwrap_or_else(|| panic!("{file} must extract"));
        for text in kept {
            assert!(s.contains(text), "{file}: {text} preserved: '{s}'");
        }
        for text in dropped {
            assert!(!s.contains(text), "{file}: body {text} dropped: '{s}'");
        }
    }
}

#[test]
fn python_one_line_def_keeps_its_signature_row() {
    let src = "def f(): return 1\n\ndef g():\n    return 2\n";
    let s = extract(src, "one.py").expect("must extract");
    assert!(
        s.contains("def f(): return 1"),
        "one-liner signature dropped: '{s}'"
    );
    assert!(s.contains("def g():"));
    assert!(
        !s.contains("return 2"),
        "multi-line body must still drop: '{s}'"
    );
}

// Data / config / unsupported prose have no signature grammar.
#[test]
fn data_and_unsupported_prose_formats_return_none() {
    let cases: &[(&str, &str)] = &[
        ("{\"key\":\"value\",\"count\":42}", "data.json"),
        ("// comment\n{\"a\": 1}", "tsconfig.json"),
        ("key: value\ncount: 42", "config.yaml"),
        ("name: my-app\nversion: 1.0.0", "package.yml"),
        ("[package]\nname = \"foo\"", "Cargo.toml"),
        ("[section]\nkey = value", "config.ini"),
        ("Title\n=====\n\nProse.", "docs.rst"),
    ];
    for (content, path) in cases {
        assert!(
            extract(content, path).is_none(),
            "{path} has no code signatures — must return None"
        );
    }
}

#[test]
fn unsupported_or_nonshrinking_outlines_return_none() {
    // Languages outside the first-class registry have no outline.
    for (content, path) in &[
        ("local x = 1\nfunction f() return x end\n", "a.lua"),
        ("-module(d).\nrev(L) -> L.\n", "a.erl"),
        ("CREATE TABLE t (id INT);\n", "a.sql"),
        ("# Title\n\nText\n", "README.md"),
        ("type Query { user: User }\n", "schema.graphql"),
    ] {
        assert!(
            extract(content, path).is_none(),
            "{path}: no first-class grammar → must return None (no regex fallback)"
        );
    }
}

#[test]
fn skeleton_never_grows_beyond_source() {
    // The anti-growth guard: a tree-sitter language whose file barely
    // compresses (a config-shaped `.cjs` that is one big object literal with
    // no function bodies to drop) must return None rather than an outline
    // that is not smaller than the source. Use a realistically sized input so
    // the verdict reflects real compression, not per-line gutter noise.
    let cjs = "module.exports = {\n".to_string()
        + &"  presets: [['@babel/preset-env', { targets: { node: 'current' } }]],\n  plugins: ['@babel/plugin-transform-runtime'],\n".repeat(40)
        + "};\n";
    if let Some(skeleton) = extract(&cjs, "babel.config.cjs") {
        assert!(
            skeleton.len() < cjs.len(),
            "skeleton ({} bytes) must be smaller than source ({} bytes)",
            skeleton.len(),
            cjs.len()
        );
    }
}

#[test]
fn well_structured_code_still_gets_a_skeleton_after_guard() {
    // The guard is content-driven, not extension-driven: a body-heavy file is
    // dropped, but a signature-dense file that genuinely compresses survives.
    let src = "export function add(a: number, b: number): number {\n  const sum = a + b;\n  console.log(sum);\n  return sum;\n}\n\nexport function sub(a: number, b: number): number {\n  const diff = a - b;\n  console.log(diff);\n  return diff;\n}\n";
    let s = extract(src, "math.ts").expect("dense TS must still extract");
    assert!(s.len() < src.len(), "skeleton must compress");
    assert!(s.contains("add") && s.contains("sub"));
}

#[test]
fn tree_sitter_code_extracts_and_drops_bodies() {
    // A body-bearing source in a tree-sitter language compresses and extracts.
    assert!(extract(
        "export function add(a: number, b: number): number {\n  const sum = a + b;\n  return sum;\n}\n",
        "math.ts"
    )
    .is_some());
}

/// One-line declarations outline to the source itself plus gutters.
#[test]
fn an_outline_of_one_line_declarations_is_no_smaller_than_its_source() {
    let src = "function f(){ return 1; }\n".repeat(45_000);
    assert_eq!(
        extract_signatures_inner(&src, "big.ts"),
        Err(OutlineUnavailable::NotSmaller)
    );
}

/// A one-line bundle of 100,000 functions, after non-ASCII text: the row
/// lists every head, each cut at its own column, from one index of the row
/// (a scan from column 0 per head was quadratic in the line).
#[test]
fn a_one_line_bundle_of_a_hundred_thousand_functions_lists_every_head() {
    let source = format!(
        "var \u{e9}=1;{}\n",
        (0..100_000)
            .map(|index| format!("function f{index}(a){{return a}}"))
            .collect::<String>()
    );
    let outline = extract_signatures_inner(&source, "bundle.js").expect("outline");
    for index in [0, 1, 50_000, 99_999] {
        assert!(outline.contains(&format!("function f{index}")), "f{index}");
    }
    assert!(!outline.contains("function f100000"));
}

/// Outer attributes open a top-level item as decorators open a Python or
/// TypeScript one: their rows stay above the name row, while inner
/// attributes (`#![…]`) and doc comments are not rows of the item.
#[test]
fn rust_outer_attributes_show_above_their_top_level_item() {
    let source = "#![allow(dead_code)]\n\n/// A point.\n#[derive(Debug, Clone)]\n#[repr(C)]\npub struct Point {\n    x: i32,\n}\n\n#[inline]\npub fn run() {\n    let a = 1;\n    let b = 2;\n    let c = a + b;\n    println!(\"{c}\");\n}\n";
    let outline = extract_signatures_inner(source, "lib.rs").expect("outline");
    let rows: Vec<&str> = outline.lines().collect();
    assert!(rows.contains(&"4\t#[derive(Debug, Clone)] …"), "{outline}");
    assert!(rows.contains(&"10\t#[inline]"), "{outline}");
    assert!(!outline.contains("#![allow"), "{outline}");
    assert!(!outline.contains("A point"), "{outline}");
}
