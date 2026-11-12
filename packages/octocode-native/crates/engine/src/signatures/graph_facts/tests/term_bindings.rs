//! `term_binding` on a bare call: set where the lane's own scope reading binds
//! the callee to a parameter, a local variable or a closure; never from the
//! name, and never where a bare call names a declaration.

/// The 1-based lines of the bare `handler` calls, each with its fact.
fn handler_calls(source: &str, path: &str) -> Vec<(u32, bool)> {
    let facts = crate::signatures::extract_graph_facts(source, path).expect("graph facts");
    facts
        .calls
        .iter()
        .filter(|call| call.callee == "handler")
        .map(|call| (call.line, call.term_binding))
        .collect()
}

/// Line 2 calls a parameter, line 3 a local closure, line 4 the declaration.
const EXPECTED: [(u32, bool); 3] = [(2, true), (3, true), (4, false)];

/// oxc binds each JS/TS callee to its symbol: a parameter is a term, a
/// nested function-valued `const` is the declaration the outline lists, and
/// the top-level function is its own declaration.
#[test]
fn oxc_binds_a_parameter_as_a_term_and_a_declared_callee_to_its_declaration() {
    for (path, source) in [
        (
            "m.js",
            "function handler() { return 1; }\nfunction run(handler) { return handler(); }\nfunction local() { const handler = () => 2; return handler(); }\nfunction direct() { return handler(); }\n",
        ),
        (
            "m.ts",
            "function handler(): number { return 1; }\nfunction run(handler: () => number) { return handler(); }\nfunction local() { let handler = function () { return 2; }; return handler(); }\nfunction direct() { return handler(); }\n",
        ),
    ] {
        let facts = crate::signatures::extract_graph_facts(source, path).expect("graph facts");
        let id = |line: u32| {
            facts
                .declarations
                .iter()
                .find(|d| d.name == "handler" && d.selection_range.start.line + 1 == line)
                .map(|d| d.id.clone())
        };
        let calls = facts
            .calls
            .iter()
            .filter(|call| call.callee == "handler")
            .map(|call| (call.line, call.term_binding, call.bound_declaration.clone()))
            .collect::<Vec<_>>();
        assert!(
            id(3).is_some() && id(1).is_some(),
            "{path}: {:?}",
            facts.declarations
        );
        assert_eq!(
            calls,
            [(2, true, None), (3, false, id(3)), (4, false, id(1))],
            "{path}"
        );
    }
}

#[test]
fn a_parameter_or_local_callee_is_a_term_binding_in_every_lane_that_reads_scopes() {
    let cases: &[(&str, &str)] = &[
        (
            "m.py",
            "def handler(): return 1\ndef run(handler): return handler()\ndef local(): handler = lambda: 2; return handler()\ndef direct(): return handler()\n",
        ),
        (
            "m.go",
            "package main\nfunc run(handler func() int) int { return handler() }\nfunc local() int { handler := func() int { return 2 }; return handler() }\nfunc direct() int { return handler() }\nfunc handler() int { return 1 }\n",
        ),
        (
            "m.rs",
            "fn handler() -> i32 { 1 }\nfn run(handler: impl Fn() -> i32) -> i32 { handler() }\nfn local() -> i32 { let handler = || 2; handler() }\nfn direct() -> i32 { handler() }\n",
        ),
        (
            "m.c",
            "int handler(void) { return 1; }\nint run(int (*handler)(void)) { return handler(); }\nint local(void) { int (*handler)(void) = 0; return handler(); }\nint direct(void) { return handler(); }\n",
        ),
        (
            "m.cpp",
            "int handler() { return 1; }\ntemplate <typename F> int run(F handler) { return handler(); }\nint local() { auto handler = [] { return 2; }; return handler(); }\nint direct() { return handler(); }\n",
        ),
        (
            "m.cu",
            "int handler() { return 1; }\ntemplate <typename F> int run(F handler) { return handler(); }\nint local() { auto handler = [] { return 2; }; return handler(); }\nint direct() { return handler(); }\n",
        ),
        (
            "A.cs",
            "class A { static int handler() { return 1; }\n  static int run(System.Func<int> handler) { return handler(); }\n  static int local() { System.Func<int> handler = () => 2; return handler(); }\n  static int direct() { return handler(); } }\n",
        ),
    ];
    for (path, source) in cases {
        assert_eq!(handler_calls(source, path), EXPECTED, "{path}");
    }
}

/// A Java bare call names a method: a same-named local never shadows it.
#[test]
fn a_java_local_never_binds_a_bare_call() {
    assert_eq!(
        handler_calls(
            "class A {\n  static int run() { int handler = 3; return handler() + handler; }\n}\n",
            "A.java",
        ),
        [(2, false)]
    );
}

/// A written type in a pattern and a block-scope prototype bind no value.
#[test]
fn types_in_patterns_and_function_prototypes_bind_no_value() {
    let python = crate::signatures::extract_graph_facts(
        "class Handler: pass\ndef typed(h: Handler):\n    return Handler()\n",
        "m.py",
    )
    .expect("graph facts");
    assert!(
        python.calls.iter().all(|call| !call.term_binding),
        "{:?}",
        python.calls
    );
    let rust = crate::signatures::extract_graph_facts(
        "struct Wrapper(i32);\nfn f(w: Wrapper) -> i32 { let Wrapper(inner) = w; let _ = Wrapper(3); inner }\n",
        "m.rs",
    )
    .expect("graph facts");
    assert!(
        rust.calls.iter().all(|call| !call.term_binding),
        "{:?}",
        rust.calls
    );
    assert_eq!(
        handler_calls(
            "int handler(void) { return 1; }\nint declared(void) { int handler(void); return handler(); }\n",
            "m.c",
        ),
        [(2, false)]
    );
}
