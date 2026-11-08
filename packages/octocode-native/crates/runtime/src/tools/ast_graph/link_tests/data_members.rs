//! Data members (fields, properties, enum members, member constants) are
//! members of their type: a bare call never reaches one, a call through a
//! receiver or a type still does, a callable member wins a name tie with a
//! data member of its type, and a data member never moves the ID of a
//! same-named method.
use super::*;

const GO_MOD: (&str, &str) = ("go.mod", "module example.com/app\n");

/// Go's builtin `append` is no call of a struct field named `append`; a
/// call through a value of the struct still reaches its func-typed field.
#[test]
fn a_bare_go_call_never_reaches_a_struct_field() {
    let temp = fixture(&[
        GO_MOD,
        (
            "a.go",
            "package app\n\ntype T struct {\n\tappend func(int) int\n\tcb     func()\n}\n\nfunc use(xs []int) []int { return append(xs, 1) }\n\nfunc call(s T) { s.cb() }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(call_edges(root, "a.go#use"), []);
    assert_eq!(
        call_edges(root, "a.go#call"),
        [edge("a.go#T.cb", "receiver-type", "high")]
    );
}

/// A C bare call is no call of a header struct's field of that name.
#[test]
fn a_bare_c_call_never_reaches_a_struct_field() {
    let temp = fixture(&[
        (
            "s.h",
            "struct ops {\n    int (*compute)(int);\n    int scale;\n};\n",
        ),
        (
            "u.c",
            "#include \"s.h\"\n\nint f(void) { return compute(1) + scale(2); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(call_edges(root, "u.c#f"), []);
}

/// A Java accessor named as its field: the call is the method's, once, at
/// high confidence, bare or through a receiver; the method keeps the ID it
/// had before fields were declarations.
#[test]
fn a_java_method_wins_the_name_of_its_field() {
    let temp = fixture(&[(
        "src/main/java/a/Person.java",
        "package a;\n\npublic class Person {\n    private final String name;\n    Person(String name) { this.name = name; }\n    public String name() { return name; }\n    String greet() { return name(); }\n    static String of(Person p) { return p.name(); }\n}\n",
    )]);
    let root = ingested(&temp);
    let method = "src/main/java/a/Person.java#Person.name";
    assert_eq!(
        call_edges(root, "src/main/java/a/Person.java#Person.greet"),
        [edge(method, "local", "high")]
    );
    assert_eq!(
        call_edges(root, "src/main/java/a/Person.java#Person.of")
            .into_iter()
            .map(|(id, _, _)| id)
            .collect::<Vec<_>>(),
        [method]
    );
}

/// A Rust getter named as its field keeps its plain ID: the field takes the
/// line suffix.
#[test]
fn a_rust_getter_keeps_its_id_beside_its_field() {
    let temp = fixture(&[
        (
            "Cargo.toml",
            "[package]\nname = \"buf\"\nversion = \"0.1.0\"\n",
        ),
        (
            "src/lib.rs",
            "pub struct Buf {\n    len: usize,\n}\nimpl Buf {\n    pub fn len(&self) -> usize { self.len }\n}\npub fn size(b: &Buf) -> usize { b.len() }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "src/lib.rs#size"),
        [edge("src/lib.rs#Buf.len", "receiver-type", "high")]
    );
    let field = ask(root, "node", Some("src/lib.rs#Buf.len@2"));
    assert_eq!(field["node"]["id"], "src/lib.rs#Buf.len@2", "{field}");
}

/// "A callable of the same type wins" compares the type, not its name: a
/// same-named interface of another file declaring the member as a method
/// leaves this file's property member the call's only target.
#[test]
fn a_same_named_type_elsewhere_never_shadows_a_data_member() {
    let temp = fixture(&[
        (
            "src/a/types.ts",
            "export interface Type {\n  getTypes(): Promise<Type[]>;\n}\n",
        ),
        (
            "src/s/types.ts",
            "export interface Type {\n  getTypes: { (): Type[] };\n}\n",
        ),
        (
            "src/s/use.ts",
            "import type { Type } from \"./types\";\nexport function use(t: Type) { return t.getTypes(); }\n",
        ),
        GO_MOD,
        (
            "a/a.go",
            "package a\n\ntype Config struct {\n\tValidate func() error\n}\n\nfunc Use(c Config) error { return c.Validate() }\n",
        ),
        (
            "b/b.go",
            "package b\n\ntype Config struct{}\n\nfunc (c Config) Validate() error { return nil }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "src/s/use.ts#use"),
        [edge(
            "src/s/types.ts#Type.getTypes",
            "receiver-type",
            "high"
        )]
    );
    assert_eq!(
        call_edges(root, "a/a.go#Use"),
        [edge("a/a.go#Config.Validate", "receiver-type", "high")]
    );
}

/// A data member whose declared type is a function type is callable: in a
/// language with an implicit receiver (Scala, C#, C++) a bare call inside
/// its type invokes it. A value of another type (`m(1)` on a `Map`) is no
/// function, and a bare call outside the type never reaches the member.
#[test]
fn a_function_typed_data_member_is_called_by_its_bare_name_in_its_type() {
    let temp = fixture(&[
        (
            "a.scala",
            "case class Lazy[A](f: () => A) {\n  private lazy val resolved: Function0[A] = f\n  val inc: Int => Int = _ + 1\n  val m: Map[Int, Int] = Map.empty\n  def apply(): A = resolved()\n  def next(x: Int) = inc(x)\n  def at(k: Int) = m(k)\n}\n",
        ),
        (
            "W.cs",
            "using System;\n\nclass W\n{\n    Action cb;\n    Func<int> make;\n    int n;\n    void Run() { cb(); }\n    int Make() { return make(); }\n}\n",
        ),
        (
            "s.cpp",
            "#include <functional>\nstruct S {\n    std::function<void()> cb;\n    int (*fp)(int);\n    int n;\n    void run() { cb(); }\n    int go() { return fp(1); }\n};\nint free_call() { return fp(2); }\n",
        ),
        (
            "k.ts",
            "export class K {\n  cb: () => void = () => {};\n  run() { this.cb(); }\n}\nexport function bare() { cb(); }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "a.scala#Lazy.apply"),
        [edge("a.scala#Lazy.resolved", "local", "high")]
    );
    assert_eq!(
        call_edges(root, "a.scala#Lazy.next"),
        [edge("a.scala#Lazy.inc", "local", "high")]
    );
    assert_eq!(call_edges(root, "a.scala#Lazy.at"), []);
    assert_eq!(
        call_edges(root, "W.cs#W.Run"),
        [edge("W.cs#W.cb", "local", "high")]
    );
    assert_eq!(
        call_edges(root, "W.cs#W.Make"),
        [edge("W.cs#W.make", "local", "high")]
    );
    assert_eq!(
        call_edges(root, "s.cpp#S.run"),
        [edge("s.cpp#S.cb", "local", "high")]
    );
    assert_eq!(
        call_edges(root, "s.cpp#S.go"),
        [edge("s.cpp#S.fp", "local", "high")]
    );
    assert_eq!(call_edges(root, "s.cpp#free_call"), []);
    assert_eq!(
        call_edges(root, "k.ts#K.run"),
        [edge("k.ts#K.cb", "local", "high")]
    );
    assert_eq!(call_edges(root, "k.ts#bare"), []);
}
