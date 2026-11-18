//! A bare call of a parameter, a local variable or a closure calls that value:
//! it links to no declaration of the same name and stays unresolved as
//! `local-binding`, while a call of the declaration itself still links. The
//! fact comes from each lane's own scope reading (oxc semantics for JS/TS,
//! the lexical bindings for Python, Go, Rust, C-family and C#, Scala's term
//! scopes), never from the name. Where oxc binds the callee to a declaration
//! the outline lists (a nested `const f = () => ..`), the call links that one.
use super::*;

/// The callers of `target`, by id, at any confidence.
fn caller_ids(root: &Path, target: &str) -> Vec<String> {
    caller_edges(root, target)
        .into_iter()
        .map(|(id, _, _)| id)
        .collect()
}

/// The unresolved call rows named `name`, by reason, sorted.
fn call_reasons(root: &Path, name: &str) -> Vec<String> {
    let out = ask_with(root, "unresolved", None, |o| {
        o.edges = vec!["calls".into()];
    });
    assert_eq!(out.get("error"), None, "{out}");
    let mut reasons = out["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["targetName"] == name)
        .map(|row| row["reason"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    reasons.sort();
    reasons
}

/// `handler` is called through a parameter and a local closure, and once as
/// the declaration: only the last links, the others are `local-binding`.
fn assert_local_calls_stay_unlinked(files: &[(&str, &str)], target: &str, direct: &[&str]) {
    let temp = fixture(files);
    let root = ingested(&temp);
    assert_eq!(caller_ids(root, target), direct, "{target}");
    assert_eq!(
        call_reasons(root, "handler"),
        ["local-binding", "local-binding"],
        "{target}"
    );
}

/// A parameter stays `local-binding`; a nested function-valued binding the
/// outline lists links only that declaration, at high confidence.
#[test]
fn javascript_and_typescript_parameter_and_local_closure_calls_do_not_link_the_function() {
    for (file, source) in [
        (
            "m.js",
            "export function handler() { return 1; }\nexport function run(handler) { return handler(); }\nexport function local() { const handler = () => 2; return handler(); }\nexport function direct() { return handler(); }\n",
        ),
        (
            "m.ts",
            "export function handler(): number { return 1; }\nexport function run(handler: () => number) { return handler(); }\nexport function local() { let handler = function () { return 2; }; return handler(); }\nexport function direct() { return handler(); }\n",
        ),
    ] {
        let temp = fixture(&[(file, source)]);
        let root = ingested(&temp);
        let id = |name: &str| format!("{file}#{name}");
        assert_eq!(
            caller_edges(root, &id("handler")),
            [edge(&id("direct"), "local", "high")]
        );
        assert_eq!(
            caller_edges(root, &id("local.handler")),
            [edge(&id("local"), "local", "high")]
        );
        assert_eq!(call_reasons(root, "handler"), ["local-binding"], "{file}");
    }
}

/// `new C()` of a parameter constructs what the caller passed, and a JSX tag
/// of a prop renders it: neither is the same-named class or component.
#[test]
fn typescript_constructed_and_rendered_parameters_do_not_link_the_declaration() {
    let temp = fixture(&[(
        "m.tsx",
        "export class Handler {}\nexport function View() { return null; }\nexport function make(Handler: new () => object) { return new Handler(); }\nexport function build() { return new Handler(); }\nexport function Wrap({ View }: { View: () => null }) { return <View />; }\nexport function Page() { return <View />; }\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(caller_ids(root, "m.tsx#Handler"), ["m.tsx#build"]);
    assert_eq!(caller_ids(root, "m.tsx#View"), ["m.tsx#Page"]);
    assert_eq!(call_reasons(root, "Handler"), ["local-binding"]);
    assert_eq!(call_reasons(root, "View"), ["local-binding"]);
}

#[test]
fn python_parameter_and_local_lambda_calls_do_not_link_the_function() {
    assert_local_calls_stay_unlinked(
        &[(
            "m.py",
            "def handler():\n    return 1\n\ndef run(handler):\n    return handler()\n\ndef local():\n    handler = lambda: 2\n    return handler()\n\ndef direct():\n    return handler()\n",
        )],
        "m.py#handler",
        &["m.py#direct"],
    );
}

/// A parameter's written type binds no name: `Handler()` beside `h: Handler`
/// still constructs the class.
#[test]
fn python_parameter_type_is_no_local_binding() {
    let temp = fixture(&[(
        "m.py",
        "class Handler:\n    pass\n\ndef typed(h: Handler):\n    return Handler()\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(caller_ids(root, "m.py#Handler"), ["m.py#typed"]);
}

#[test]
fn go_parameter_and_local_closure_calls_do_not_link_the_function() {
    assert_local_calls_stay_unlinked(
        &[
            ("go.mod", "module example.com/app\n"),
            (
                "main.go",
                "package main\n\nfunc handler() int { return 1 }\n\nfunc run(handler func() int) int { return handler() }\n\nfunc local() int {\n\thandler := func() int { return 2 }\n\treturn handler()\n}\n\nfunc direct() int { return handler() }\n",
            ),
        ],
        "main.go#handler",
        &["main.go#direct"],
    );
}

#[test]
fn rust_parameter_and_local_closure_calls_do_not_link_the_function() {
    assert_local_calls_stay_unlinked(
        &[
            CARGO_APP,
            (
                "src/lib.rs",
                "pub fn handler() -> i32 { 1 }\npub fn run(handler: impl Fn() -> i32) -> i32 { handler() }\npub fn local() -> i32 { let handler = || 2; handler() }\npub fn direct() -> i32 { handler() }\n",
            ),
        ],
        "src/lib.rs#handler",
        &["src/lib.rs#direct"],
    );
}

/// A block-scope prototype declares the function itself: its call links.
#[test]
fn c_parameter_and_local_pointer_calls_do_not_link_the_function() {
    assert_local_calls_stay_unlinked(
        &[(
            "m.c",
            "int handler(void) { return 1; }\nint run(int (*handler)(void)) { return handler(); }\nint local(void) { int (*handler)(void) = 0; return handler(); }\nint direct(void) { return handler(); }\nint declared(void) { int handler(void); return handler(); }\n",
        )],
        "m.c#handler",
        &["m.c#declared", "m.c#direct"],
    );
}

#[test]
fn cpp_and_cuda_parameter_and_local_lambda_calls_do_not_link_the_function() {
    for file in ["m.cpp", "m.cu"] {
        let target = format!("{file}#handler");
        let direct = format!("{file}#direct");
        assert_local_calls_stay_unlinked(
            &[(
                file,
                "int handler() { return 1; }\ntemplate <typename F> int run(F handler) { return handler(); }\nint local() { auto handler = [] { return 2; }; return handler(); }\nint direct() { return handler(); }\n",
            )],
            &target,
            &[direct.as_str()],
        );
    }
}

#[test]
fn csharp_parameter_and_local_delegate_calls_do_not_link_the_method() {
    assert_local_calls_stay_unlinked(
        &[(
            "A.cs",
            "class A {\n  static int handler() { return 1; }\n  static int run(System.Func<int> handler) { return handler(); }\n  static int local() { System.Func<int> handler = () => 2; return handler(); }\n  static int direct() { return handler(); }\n}\n",
        )],
        "A.cs#A.handler",
        &["A.cs#A.direct"],
    );
}

/// Scala records the fact from its own term scopes: the same reason.
#[test]
fn scala_parameter_call_does_not_link_the_def() {
    let temp = fixture(&[(
        "a.scala",
        "object A {\n  def handler(): Int = 1\n  def run(handler: () => Int): Int = handler()\n  def direct(): Int = handler()\n}\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(caller_ids(root, "a.scala#A.handler"), ["a.scala#A.direct"]);
    assert_eq!(call_reasons(root, "handler"), ["local-binding"]);
}

/// A bare Java call names a method only: a same-named local never shadows it.
#[test]
fn java_local_variable_does_not_shadow_a_method_call() {
    let temp = fixture(&[(
        "A.java",
        "class A {\n  static int handler() { return 1; }\n  static int run() { int handler = 3; return handler() + handler; }\n}\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(caller_ids(root, "A.java#A.handler"), ["A.java#A.run"]);
    assert_eq!(call_reasons(root, "handler"), Vec::<String>::new());
}
