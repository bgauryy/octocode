//! Calls the syntax can bind but an earlier form lost, and the reason a call that
//! stays unlinked gives: it must be true of what the file says.
use super::*;

const P_H: &str = "struct P { int y() const { return 1; } };\nnamespace ns { int free_fn(); }\n";

fn cpp(body: &str) -> Fixture {
    let source = format!(
        "#include \"x.h\"\nnamespace al = ns;\nint ns::free_fn() {{ return 3; }}\n{body}\n"
    );
    fixture(&[("x.h", P_H), ("x.cc", &source)])
}

/// `(*p).y()` is `p->y()`: the same receiver, the same target.
#[test]
fn cpp_dereferenced_receiver_binds_as_the_arrow_form_does() {
    let temp = cpp(
        "int arrow(P* p) { return p->y(); }\nint star(P* p) { return (*p).y(); }\nint deep(P* p) { return (*(p)).y(); }",
    );
    let root = ingested(&temp);
    let arrow = call_edges(root, "x.cc#arrow");
    assert!(!arrow.is_empty());
    assert_eq!(call_edges(root, "x.cc#star"), arrow);
    assert_eq!(call_edges(root, "x.cc#deep"), arrow);
}

/// `auto a = P{};` and `auto a = P(1);` name `a`'s type as a `new` would.
#[test]
fn cpp_auto_from_a_written_type_binds_the_receiver() {
    let temp = cpp(
        "int arrow(P* p) { return p->y(); }\nint braces() { auto a = P{}; return a.y(); }\nint call() { auto a = P(); return a.y(); }",
    );
    let root = ingested(&temp);
    let arrow = call_edges(root, "x.cc#arrow");
    assert!(!arrow.is_empty());
    for caller in ["x.cc#braces", "x.cc#call"] {
        let edges = call_edges(root, caller);
        assert!(
            edges.iter().any(|(id, _, _)| *id == arrow[0].0),
            "{caller}: {edges:?}"
        );
    }
}

/// A namespace alias names the namespace: no receiver is unknown here.
#[test]
fn cpp_namespace_alias_calls_the_namespace_member() {
    let temp = cpp("int run() { return al::free_fn(); }\nint direct() { return ns::free_fn(); }");
    let root = ingested(&temp);
    let direct = call_edges(root, "x.cc#direct");
    assert!(!direct.is_empty());
    assert_eq!(call_edges(root, "x.cc#run"), direct);
}

/// `const f = NS.alpha; f()` calls what `f` is bound to, in its scope; a `let`,
/// which may be rebound, or another function's `f` does not.
#[test]
fn typescript_const_alias_of_a_namespace_member_binds_the_call() {
    let temp = fixture(&[
        ("lib/a.ts", "export function alpha() { return 1; }\n"),
        (
            "use.ts",
            "import * as NS from \"./lib/a\";\nimport { alpha } from \"./lib/a\";\nexport function run() {\n  const f = NS.alpha;\n  f();\n}\nexport function chain() {\n  const f = alpha;\n  const g = f;\n  g();\n}\nexport function rebound() {\n  let f = NS.alpha;\n  f();\n}\nexport function outside() {\n  f();\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    let target = "lib/a.ts#alpha";
    let has_alpha = |caller: &str| {
        call_edges(root, caller)
            .iter()
            .any(|(id, _, _)| id == target)
    };
    assert!(has_alpha("use.ts#run"));
    assert!(has_alpha("use.ts#chain"));
    assert!(!has_alpha("use.ts#rebound"));
    assert!(!has_alpha("use.ts#outside"));
}

/// `mylib::f()` through the crate's name with no `use` of it.
#[test]
fn rust_crate_root_path_call_binds_with_no_use() {
    let temp = fixture(&[
        (
            "lib/Cargo.toml",
            "[package]\nname = \"my-lib\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("lib/src/lib.rs", "pub fn f() {}\n"),
        (
            "app/Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nmy-lib = { path = \"../lib\" }\n",
        ),
        ("app/src/main.rs", "fn main() {\n    my_lib::f();\n}\n"),
    ]);
    let root = ingested(&temp);
    let edges = call_edges(root, "app/src/main.rs#main");
    assert!(
        edges.iter().any(|(id, _, _)| id == "lib/src/lib.rs#f"),
        "{edges:?}"
    );
}
