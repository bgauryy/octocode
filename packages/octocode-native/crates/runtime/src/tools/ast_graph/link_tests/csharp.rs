//! C#: a simple method name binds the enclosing class (all its partial
//! parts) and its base classes, then a `using static` type; `base.M()` the
//! base class; types bind lexically, through a `using` alias, the caller's
//! namespace chain, then `using` namespaces.
use super::*;

const BASE: (&str, &str) = (
    "src/Core/Base.cs",
    "namespace Acme.Core\n{\n    public class Base\n    {\n        public virtual int Over() { return 1; }\n        public int BaseOp() { return 2; }\n    }\n}\n",
);
const UTIL: [(&str, &str); 3] = [
    (
        "src/Util/Strings.cs",
        "namespace Acme.Util;\n\npublic class Strings\n{\n    public static int Pad(int x) { return x; }\n    public int Inst() { return 1; }\n}\n",
    ),
    (
        "src/Util/Maths.cs",
        "namespace Acme.Util;\n\npublic static class Maths\n{\n    public static int Square(int x) { return x * x; }\n}\n",
    ),
    (
        "src/Util/Helper.cs",
        "namespace Acme.Util;\n\npublic class Helper\n{\n    public int Help() { return 1; }\n}\n",
    ),
];
const OTHER_HELPER: (&str, &str) = (
    "src/Other/Helper.cs",
    "namespace Acme.Other;\n\npublic class Helper\n{\n    public int Help() { return 2; }\n    public int Stray() { return 3; }\n}\n",
);

fn csharp(extra: &[(&'static str, &'static str)]) -> crate::tools::test_support::Fixture {
    let mut files = vec![BASE, OTHER_HELPER];
    files.extend_from_slice(&UTIL);
    files.extend_from_slice(extra);
    fixture(&files)
}

#[test]
fn base_this_and_bare_calls_bind_the_class_and_its_bases() {
    let temp = csharp(&[(
        "src/Core/Child.cs",
        "namespace Acme.Core;\n\npublic class Child : Base\n{\n    public override int Over() { return base.Over() + this.BaseOp() + BaseOp() + Stray(); }\n    class Inner { int In() { return BaseOp() + Local(); int Local() => 1; } }\n    int Local() => 2;\n}\n",
    )]);
    let root = ingested(&temp);
    let base = "src/Core/Base.cs#Acme.Core.Base";
    assert_eq!(
        callees(root, "src/Core/Child.cs#Child.Over"),
        [
            to(&format!("{base}.BaseOp"), "inherited"),
            to(&format!("{base}.Over"), "inherited"),
        ]
    );
    // A local function shadows the outer class's same-named method.
    assert_eq!(
        callees(root, "src/Core/Child.cs#Child.Inner.In"),
        [
            to(&format!("{base}.BaseOp"), "inherited"),
            to("src/Core/Child.cs#Child.Inner.In.Local", "local"),
        ]
    );
}

/// Partial parts of one class (same namespace and name) share members; a
/// same-named class of another namespace is a different class.
#[test]
fn partial_classes_are_one_class_across_files() {
    let temp = csharp(&[
        (
            "src/Core/Child.cs",
            "namespace Acme.Core;\n\npublic partial class Child\n{\n    int A() { return Other(); }\n}\n",
        ),
        (
            "src/Core/Child.Part.cs",
            "namespace Acme.Core\n{\n    public partial class Child : Base\n    {\n        int Other() { return BaseOp(); }\n    }\n}\n",
        ),
        (
            "src/Core/Elsewhere.cs",
            "namespace Acme.Elsewhere;\n\npublic partial class Child\n{\n    int Other() { return 0; }\n    int B() { return BaseOp(); }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        callees(root, "src/Core/Child.cs#Child.A"),
        [to("src/Core/Child.Part.cs#Acme.Core.Child.Other", "local")]
    );
    assert_eq!(
        callees(root, "src/Core/Child.Part.cs#Acme.Core.Child.Other"),
        [to("src/Core/Base.cs#Acme.Core.Base.BaseOp", "inherited")]
    );
    assert_eq!(callees(root, "src/Core/Elsewhere.cs#Child.B"), []);
}

/// `using N;` and the caller's namespace bring types in; an alias names its
/// type; `using static T;` brings `T`'s members in; another namespace's
/// same-named type stays out.
#[test]
fn usings_and_namespaces_bind_types() {
    let temp = csharp(&[(
        "src/App/Use.cs",
        "using Acme.Util;\nusing static Acme.Util.Maths;\nusing H = Acme.Other.Helper;\n\nnamespace Acme.App;\n\npublic class Use\n{\n    private readonly H helper = new H();\n    int A() { return Strings.Pad(1) + Square(2) + helper.Help(); }\n    int B() { var s = new Strings(); return s.Inst() + new Helper().Help(); }\n}\n",
    )]);
    let root = ingested(&temp);
    let other = "src/Other/Helper.cs#Helper";
    assert_eq!(
        callees(root, "src/App/Use.cs#Use.A"),
        [
            to(&format!("{other}.Help"), "receiver-type"),
            to("src/Util/Maths.cs#Maths.Square", "import"),
            to("src/Util/Strings.cs#Strings.Pad", "type-qualified"),
        ]
    );
    assert_eq!(
        callees(root, "src/App/Use.cs#Use"),
        [to(other, "constructs:import")]
    );
    let util = "src/Util/Helper.cs#Helper";
    assert_eq!(
        callees(root, "src/App/Use.cs#Use.B"),
        [
            to(util, "constructs:import-glob"),
            to(&format!("{util}.Help"), "receiver-type"),
            to("src/Util/Strings.cs#Strings", "constructs:import-glob"),
            to("src/Util/Strings.cs#Strings.Inst", "receiver-type"),
        ]
    );
}

/// Without a `using`, a type or static method of another namespace is not
/// in scope; an extension method's receiver type is unknown; a literal
/// receiver names nothing.
#[test]
fn names_out_of_scope_stay_unlinked() {
    let temp = csharp(&[
        (
            "src/Util/Extensions.cs",
            "namespace Acme.Util;\n\npublic static class Extensions\n{\n    public static int Shout(this string s) { return s.Length; }\n}\n",
        ),
        (
            "src/App/Plain.cs",
            "namespace Acme.App;\n\npublic class Plain\n{\n    int A() { return Square(2) + Help() + \"Pad\".Shout() + new Strings().Inst(); }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(callees(root, "src/App/Plain.cs#Plain.A"), []);
}

/// Interface and struct receivers bind their declared type; a nested type
/// path binds the nested type; a property read is no call.
#[test]
fn interfaces_structs_and_nested_types_bind_their_type() {
    let temp = csharp(&[(
        "src/Core/Shapes.cs",
        "namespace Acme.Core;\n\npublic interface IShape { int Area(); }\npublic struct Square : IShape { public int Area() => 1; public int Side { get; set; } }\npublic class Outer { public class Inner { public static int Make() => 1; } }\npublic class Use { int U(IShape s, Square q) { return s.Area() + q.Area() + Outer.Inner.Make() + q.Side; } }\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(
        callees(root, "src/Core/Shapes.cs#Use.U"),
        [
            to("src/Core/Shapes.cs#IShape.Area", "receiver-type"),
            to("src/Core/Shapes.cs#Outer.Inner.Make", "type-qualified"),
            to("src/Core/Shapes.cs#Square.Area", "receiver-type"),
        ]
    );
}
