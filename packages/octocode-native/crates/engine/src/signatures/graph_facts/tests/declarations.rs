//! Declarations: containers, keyword names, kinds and export visibility.

use super::*;

#[test]
fn rust_macro_arms_do_not_declare_attribute_or_body_identifiers() {
    for body in [
        "#[macro_export]\n$item",
        "#[allow(dead_code)]\n$item",
        "body_identifier()",
    ] {
        let source = format!(
            "macro_rules! wrapper {{ ($item:item) => {{ {body} }}; }}\n\
             macro_rules! value {{ () => {{ 1 }}; }}\n\
             fn use_macro() {{ value!(); }}\n"
        );
        let graph = extract_graph_facts_with_metadata(&source, "macros.rs")
            .expect("graph facts")
            .facts;
        let outline = crate::signatures::extract_declarations_document(&source, "macros.rs")
            .expect("declarations");
        for document in [&graph, &outline] {
            let names: Vec<_> = document
                .declarations
                .iter()
                .map(|declaration| declaration.name.as_str())
                .collect();
            assert_eq!(names, ["wrapper", "value", "use_macro"], "{body}");
        }
        assert!(
            graph
                .calls
                .iter()
                .any(|call| call.caller == "use_macro" && call.callee == "value"),
            "macro invocation: {:?}",
            graph.calls
        );
    }
}

#[test]
fn rust_nested_macro_definitions_and_invocations_keep_their_owner() {
    let source = "fn use_macro() {\n\
                  macro_rules! local_value { () => { 1 }; }\n\
                  local_value!();\n\
                  }\n";
    let graph = extract_graph_facts_with_metadata(source, "nested-macro.rs")
        .expect("graph facts")
        .facts;
    let caller = graph
        .declarations
        .iter()
        .find(|declaration| declaration.name == "use_macro")
        .expect("function");
    let nested = graph
        .declarations
        .iter()
        .find(|declaration| declaration.name == "local_value")
        .expect("nested macro");
    assert_eq!(nested.kind, "macro");
    assert_eq!(nested.parent.as_deref(), Some(caller.id.as_str()));
    assert_eq!(graph.declarations.len(), 2);
    assert!(graph.calls.iter().any(|call| {
        call.callee == "local_value" && call.caller_id.as_deref() == Some(caller.id.as_str())
    }));
}

#[cfg(feature = "tree-sitter-cpp")]
#[test]
fn cpp_class_owns_its_method_declarations() {
    let value = facts(
        "class Fixture { public: int target(int value) { return value; } };",
        "fixture.cpp",
    );
    let declarations = value["declarations"].as_array().unwrap();
    let class = declarations
        .iter()
        .find(|item| item["name"] == "Fixture")
        .expect("class declaration");
    let method = declarations
        .iter()
        .find(|item| item["name"] == "target")
        .expect("method declaration");
    assert_eq!(class["kind"], "class");
    assert_eq!(method["parent"], class["id"]);
    assert!(
        value["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| edge["relation"] == "contains"
                && edge["from"] == class["id"]
                && edge["to"] == method["id"])
    );
}

/// Body-less declarations (a Rust trait's required method or an extern
/// fn, a Go interface method, a C++ member declaration) are declarations
/// of their container, as the bodied ones are.
#[test]
fn body_less_signatures_are_declarations_of_their_container() {
    let member = |source: &str, path: &str, container: &str, name: &str, kind: &str| {
        let value = facts(source, path);
        let declarations = value["declarations"].as_array().unwrap();
        let parent = declarations
            .iter()
            .find(|item| item["name"] == container)
            .map(|item| item["id"].clone());
        let found = declarations
            .iter()
            .find(|item| item["name"] == name)
            .unwrap_or_else(|| panic!("{path} {name}: {declarations:?}"));
        assert_eq!(found["kind"], kind, "{path} {name}");
        if let Some(parent) = parent {
            assert_eq!(found["parent"], parent, "{path} {name}");
        }
    };
    let rust = "pub trait T { fn req(&self) -> u8; fn def(&self) {} }\nextern \"C\" { fn c_fn(x: i32); }\n";
    member(rust, "a.rs", "T", "req", "method");
    member(rust, "a.rs", "T", "def", "method");
    member(rust, "a.rs", "", "c_fn", "function");
    let go =
        "package p\ntype Reader interface {\n\tRead(p []byte) (int, error)\n\tClose() error\n}\n";
    member(go, "a.go", "Reader", "Read", "method");
    member(go, "a.go", "Reader", "Close", "method");
}

#[cfg(feature = "tree-sitter-cpp")]
#[test]
fn cpp_class_owns_its_member_function_declarations() {
    let value = facts(
        "class A { public: virtual void run() = 0; void go(); int field; };",
        "a.cpp",
    );
    let declarations = value["declarations"].as_array().unwrap();
    let class = declarations
        .iter()
        .find(|item| item["name"] == "A")
        .expect("class declaration");
    for name in ["run", "go"] {
        let method = declarations
            .iter()
            .find(|item| item["name"] == name)
            .unwrap_or_else(|| panic!("{name}: {declarations:?}"));
        assert_eq!(method["kind"], "method", "{name}");
        assert_eq!(method["parent"], class["id"], "{name}");
    }
    let field = declarations
        .iter()
        .find(|item| item["name"] == "field")
        .unwrap_or_else(|| panic!("field: {declarations:?}"));
    assert_eq!(field["kind"], "property");
    assert_eq!(field["parent"], class["id"]);
}

/// A data-member case: path, source, the type, its `(member, kind, 1-based
/// line)` rows, and the locals that stay out.
type MemberCase = (
    &'static str,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str, u32)],
    &'static [&'static str],
);

/// Every data member a type declares is a declaration of that type, in the
/// outline and the graph facts alike: each declarator of a multi-name field
/// (`int a, b;`), named on its own line, with a constant field's kind kept.
/// Locals of a body and members of a literal stay out.
#[test]
fn data_members_are_declarations_of_their_type() {
    let cases: &[MemberCase] = &[
        #[cfg(feature = "tree-sitter-c-sharp")]
        (
            "Square.cs",
            "public class Square\n{\n    public int Side;\n    private int a, b;\n    public event System.EventHandler Changed;\n    public const int Max = 4, Min = 0;\n    public int Area() { int local = Side; return local; }\n}\n",
            "Square",
            &[
                ("Side", "property", 3),
                ("a", "property", 4),
                ("b", "property", 4),
                ("Changed", "property", 5),
                ("Max", "constant", 6),
                ("Min", "constant", 6),
            ],
            &["local"],
        ),
        (
            "Square.java",
            "public class Square {\n    public int side;\n    private int a, b;\n    static final int MAX = 1, MIN = 0;\n    int area() { int local = side; return local; }\n}\n",
            "Square",
            &[
                ("side", "property", 2),
                ("a", "property", 3),
                ("b", "property", 3),
                ("MAX", "constant", 4),
                ("MIN", "constant", 4),
            ],
            &["local"],
        ),
        (
            "point.c",
            "struct P {\n    int x;\n    int *p, y[3];\n};\nint main(void) { int local = 0; return local; }\n",
            "P",
            &[
                ("x", "property", 2),
                ("p", "property", 3),
                ("y", "property", 3),
            ],
            &["local"],
        ),
        #[cfg(feature = "tree-sitter-cpp")]
        (
            "square.cpp",
            "class Square {\npublic:\n    int side;\n    int a, b;\n    int area() { int local = side; return local; }\n};\n",
            "Square",
            &[
                ("side", "property", 3),
                ("a", "property", 4),
                ("b", "property", 4),
            ],
            &["local"],
        ),
        (
            "square.go",
            "package p\n\ntype Square struct {\n\tSide int\n\tA, B int\n}\n\nfunc use() int { s := Square{Side: 1}; return s.Side }\n",
            "Square",
            &[
                ("Side", "property", 4),
                ("A", "property", 5),
                ("B", "property", 5),
            ],
            &["s"],
        ),
        (
            "square.rs",
            "pub struct Square {\n    pub side: i32,\n    other: u8,\n}\nfn make() -> Square { let local = 1; Square { side: local, other: 0 } }\n",
            "Square",
            &[("side", "property", 2), ("other", "property", 3)],
            &["local"],
        ),
        (
            "square.py",
            "class Square:\n    side: int = 1\n    LIMIT = 3\n    def area(self):\n        local = self.side\n        return local\n",
            "Square",
            &[("side", "property", 2), ("LIMIT", "constant", 3)],
            &["local"],
        ),
        #[cfg(feature = "tree-sitter-scala")]
        (
            "Square.scala",
            "class Square {\n  var side: Int = 1\n  def area(): Int = { var local = side; local }\n}\n",
            "Square",
            &[("side", "variable", 2)],
            &["local"],
        ),
    ];
    assert_members(cases);
}

/// Each case's members are declared once each, as its type's members of
/// their kind on their line, in the graph facts and the outline alike, and
/// none of its locals is.
fn assert_members(cases: &[MemberCase]) {
    for (path, source, owner, members, locals) in cases {
        let graph = extract_graph_facts_with_metadata(source, path)
            .expect("graph facts")
            .facts;
        let outline =
            crate::signatures::extract_declarations_document(source, path).expect("declarations");
        for (lane, document) in [("graph", &graph), ("outline", &outline)] {
            assert_declares(lane, path, &document.declarations, owner, members, locals);
        }
    }
}

fn assert_declares(
    lane: &str,
    path: &str,
    declarations: &[crate::graph::GraphFactDeclaration],
    owner: &str,
    members: &[(&str, &str, u32)],
    locals: &[&str],
) {
    let parent = declarations
        .iter()
        .find(|declaration| declaration.name == owner)
        .unwrap_or_else(|| panic!("{lane} {path} {owner}: {declarations:?}"));
    for (name, kind, line) in members {
        let named = declarations
            .iter()
            .filter(|declaration| declaration.name == *name)
            .collect::<Vec<_>>();
        let [member] = named[..] else {
            panic!("{lane} {path} {name} once: {declarations:?}");
        };
        assert_eq!(member.kind, *kind, "{lane} {path} {name}");
        assert_eq!(
            member.selection_range.start.line + 1,
            *line,
            "{lane} {path} {name}"
        );
        assert_eq!(
            member.parent.as_deref(),
            Some(parent.id.as_str()),
            "{lane} {path} {name}"
        );
    }
    for local in locals {
        assert!(
            !declarations
                .iter()
                .any(|declaration| declaration.name == *local),
            "{lane} {path} local {local}: {declarations:?}"
        );
    }
}

/// A Go embedded field is named by its type, through a package qualifier,
/// a pointer and type arguments, as the language names it.
#[test]
fn go_embedded_fields_are_named_by_their_type() {
    assert_members(&[(
        "embed.go",
        "package p\n\nimport \"io\"\n\ntype S struct {\n\tio.Reader\n\t*Base\n\tName string\n\tList[int]\n}\n",
        "S",
        &[
            ("Reader", "property", 6),
            ("Base", "property", 7),
            ("Name", "property", 8),
            ("List", "property", 9),
        ],
        &["io", "int"],
    )]);
}

/// A Python instance attribute is declared by its first assignment through
/// `__init__`'s receiver, at any depth of `__init__`'s own body; a later
/// assignment, another object's attribute, a nested attribute, a nested
/// function's assignment and another method's assignment declare nothing.
#[test]
fn python_instance_attributes_are_declared_once_from_init() {
    assert_members(&[(
        "inst.py",
        "class Square:\n    side: int = 1\n    def __init__(this, n):\n        this.n = n\n        this.n = n + 1\n        if n:\n            this.extra: int = 2\n        other.y = 3\n        this.a.b = 4\n        def inner(me):\n            me.z = 1\n    def area(self):\n        self.cache = 1\n",
        "Square",
        &[
            ("side", "property", 2),
            ("n", "property", 4),
            ("extra", "property", 7),
        ],
        &["y", "a", "b", "z", "cache"],
    )]);
}

/// Every grammar's enum members are its enum's `enumMember`s.
#[test]
fn enum_members_are_declarations_of_their_enum() {
    let members: &[(&str, &str, u32)] = &[("A", "enumMember", 1), ("B", "enumMember", 1)];
    assert_members(&[
        (
            "en.rs",
            "enum E { A, B(u8), C { x: u8 } }\n",
            "E",
            &[
                ("A", "enumMember", 1),
                ("B", "enumMember", 1),
                ("C", "enumMember", 1),
            ],
            &[],
        ),
        (
            "En.java",
            "enum E { A, B; void f() {} }\n",
            "E",
            members,
            &[],
        ),
        #[cfg(feature = "tree-sitter-c-sharp")]
        ("En.cs", "enum E { A, B = 2 }\n", "E", members, &[]),
        ("en.c", "enum E { A, B = 2 };\n", "E", members, &[]),
        #[cfg(feature = "tree-sitter-cpp")]
        ("en.cpp", "enum class E { A, B };\n", "E", members, &[]),
        #[cfg(feature = "tree-sitter-scala")]
        (
            "En.scala",
            "enum E:\n  case A, B\n  case C(x: Int)\n",
            "E",
            &[
                ("A", "enumMember", 2),
                ("B", "enumMember", 2),
                ("C", "enumMember", 3),
            ],
            &["x"],
        ),
        ("en.ts", "enum E { A, B = 2 }\n", "E", members, &[]),
    ]);
    // The tree-sitter lane TypeScript falls back to names them as oxc does.
    let source = "enum E { A, B = 2 }\n";
    let facts = extract_graph_facts_with_metadata_with_extension(source, "en.ts", "ts")
        .expect("tree-sitter graph facts")
        .facts;
    assert_declares(
        "tree-sitter",
        "en.ts",
        &facts.declarations,
        "E",
        members,
        &[],
    );
}

#[test]
fn keyword_named_declarations_survive_outside_the_js_family() {
    let rust = declared_names(
        "struct Foo;\nimpl Foo {\n    pub fn new() -> Self { Foo }\n    pub fn build() -> Self { Foo }\n}\n",
        "a.rs",
    );
    for name in ["new", "build"] {
        assert!(
            rust.iter().any(|found| found == name),
            "rust {name}: {rust:?}"
        );
    }
    let python = declared_names(
        "def new():\n    pass\n\nclass K:\n    def case(self):\n        pass\n",
        "a.py",
    );
    for name in ["new", "case"] {
        assert!(
            python.iter().any(|found| found == name),
            "python {name}: {python:?}"
        );
    }
}

/// The tree-sitter lane (what TypeScript falls back to when oxc cannot
/// parse it) names TypeScript aliases, abstract classes and namespaces with
/// the kinds the oxc lane uses.
#[test]
fn tree_sitter_typescript_declarations_keep_their_kinds() {
    let source = "export type Id = string;\nexport abstract class Shape {}\nnamespace Geo { export const x = 1; }\n";
    let facts = extract_graph_facts_with_metadata_with_extension(source, "a.ts", "ts")
        .expect("tree-sitter graph facts")
        .facts;
    let kind_of = |name: &str| {
        facts
            .declarations
            .iter()
            .find(|declaration| declaration.name == name)
            .map(|declaration| declaration.kind.clone())
            .unwrap_or_else(|| panic!("{name}: {:?}", facts.declarations))
    };
    assert_eq!(kind_of("Id"), "type");
    assert_eq!(kind_of("Shape"), "class");
    assert_eq!(kind_of("Geo"), "namespace");
}

#[test]
fn keyword_named_class_members_stay_legal_in_clean_js() {
    let js = declared_names("class A {\n  if() {}\n  build() {}\n}\n", "a.js");
    assert!(js.iter().any(|found| found == "build"), "{js:?}");
    assert!(js.iter().any(|found| found == "if"), "{js:?}");
}

/// Only plain `pub` exports a Rust item; a restricted visibility keeps it
/// inside the crate.
#[test]
fn rust_restricted_visibility_is_not_exported() {
    let src = "pub fn open() {}\npub(crate) fn krate() {}\npub(super) fn parent() {}\npub(self) fn own() {}\npub(in crate::a) fn scoped() {}\nfn private() {}\npub\nfn split() {}\n";
    let graph = facts(src, "vis.rs");
    let exported = |name: &str| {
        graph["declarations"]
            .as_array()
            .expect("declarations")
            .iter()
            .find(|decl| decl["name"] == name)
            .unwrap_or_else(|| panic!("{name}: {graph}"))
            .get("exported")
            == Some(&Value::Bool(true))
    };
    assert!(exported("open"));
    assert!(exported("split"));
    for name in ["krate", "parent", "own", "scoped", "private"] {
        assert!(!exported(name), "{name}");
    }
}

fn kinds(source: &str, path: &str) -> Vec<(String, String, bool)> {
    extract_graph_facts_with_metadata(source, path)
        .expect("graph facts")
        .facts
        .declarations
        .iter()
        .map(|d| (d.name.clone(), d.kind.clone(), d.exported))
        .collect()
}

fn row(name: &str, kind: &str, exported: bool) -> (String, String, bool) {
    (name.to_owned(), kind.to_owned(), exported)
}

#[test]
fn rust_unions_are_declared() {
    assert_eq!(
        kinds("pub union U { a: u8 }\nunion P { b: u8 }\n", "a.rs"),
        [
            row("U", "struct", true),
            row("a", "property", false),
            row("P", "struct", false),
            row("b", "property", false)
        ]
    );
}

#[test]
fn java_annotation_types_are_declared() {
    assert_eq!(
        kinds("public @interface Ann { int v() default 0; }\n", "Ann.java"),
        [row("Ann", "interface", true)]
    );
}

#[cfg(feature = "tree-sitter-scala")]
#[test]
fn scala_enums_and_givens_are_declared() {
    let declared = kinds(
        "enum Color { case Red }\ngiven intOrd: Ord[Int] = ???\ngiven Ord[String] with { def x = 1 }\n",
        "a.scala",
    );
    assert_eq!(declared[0], row("Color", "enum", true));
    assert_eq!(declared[1], row("Red", "enumMember", false));
    assert_eq!(declared[2], row("intOrd", "constant", true));
    // An anonymous given has only the type it provides to name it.
    assert_eq!(declared[3], row("Ord[String]", "constant", true));
}
