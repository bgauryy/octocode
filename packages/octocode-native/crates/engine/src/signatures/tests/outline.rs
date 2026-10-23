//! Symbols outlines: skeletons, comment lines, over-long lines and the size cap.

use super::*;

/// A few over-long declaration lines (generated one-line structs) keep
/// the outline, verbatim; an outline made mostly of them is minified.
#[test]
fn long_declaration_lines_keep_the_outline_unless_they_dominate_it() {
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
    let long = format!("pub struct Generated {{ {fields} }}");
    assert!(long.len() > OUTLINE_MAX_LINE_CHARS);
    source.push_str(&long);
    source.push('\n');
    let outline = extract_signatures_inner(&source, "lib.rs").expect("outline");
    assert!(
        outline.contains(&long),
        "the long declaration stays verbatim"
    );
    assert!(outline.contains("pub fn run199(value: usize) -> usize"));

    let minified = format!("{long}\n{long}\npub fn tail() {{\n    let x = 1;\n}}\n");
    assert_eq!(
        extract_signatures_inner(&minified, "lib.rs"),
        Err(OutlineUnavailable::Minified)
    );
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
        extract_signatures_inner(&"x".repeat(MAX_PARSE_SIZE + 1), "big.rs"),
        Err(OutlineUnavailable::TooLarge)
    );
    assert_eq!(
        extract_signatures_inner("fn a() {}\n", "tiny.rs"),
        Err(OutlineUnavailable::NotSmaller)
    );
    assert_eq!(OutlineUnavailable::Minified.as_str(), "minified-lines");
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
        let skeleton = extract(text, path).expect("skeleton");
        let minified = crate::minify::apply::apply_content_view_minification_inner(&skeleton, path);
        views.push(skeleton);
        views.push(minified);
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

// ── size cap ──────────────────────────────────────────────────────────────
#[test]
fn oversized_input_returns_none_without_parsing() {
    let src = "function f(){ return 1; }\n".repeat(45_000); // ~1.17MB
    assert!(extract(&src, "big.ts").is_none());
}
