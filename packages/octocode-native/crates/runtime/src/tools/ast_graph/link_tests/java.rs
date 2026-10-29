//! Java: a simple method name binds the innermost enclosing class whose
//! members (declared or inherited, superclass chain before interfaces)
//! include it, then a static import; never another class of the package or
//! file. `this.m()`/`super.m()` bind the enclosing class; `T.m()` the type
//! `T`; types bind local, then single-type imports, then the package.
use super::*;

const BASE: (&str, &str) = (
    "src/com/acme/Base.java",
    "package com.acme;\n\npublic class Base {\n  public int baseOp() { return 1; }\n  public int over() { return 2; }\n  public int area() { return 3; }\n}\n",
);
const SHAPE: (&str, &str) = (
    "src/com/acme/Shape.java",
    "package com.acme;\n\ninterface Shape extends Named {\n  default int area() { return 0; }\n}\n",
);
const NAMED: (&str, &str) = (
    "src/com/acme/Named.java",
    "package com.acme;\n\ninterface Named {\n  default String label() { return \"\"; }\n}\n",
);
const STRANGER: (&str, &str) = (
    "src/com/acme/Stranger.java",
    "package com.acme;\n\nclass Stranger {\n  int helper() { return 9; }\n  static int lonely() { return 8; }\n}\n",
);

/// The same file or package declaring the name elsewhere is not in scope:
/// only the enclosing classes and what they inherit are.
#[test]
fn bare_calls_bind_the_enclosing_class_and_its_bases() {
    let temp = fixture(&[
        BASE,
        STRANGER,
        (
            "src/com/acme/Child.java",
            "package com.acme;\n\nclass Child extends Base {\n  int run() { return go() + baseOp() + helper() + lonely(); }\n  int go() { return 1; }\n  class Inner {\n    int in() { return go() + baseOp(); }\n  }\n}\n\nclass Sibling {\n  int go() { return 2; }\n}\n",
        ),
        (
            "src/com/acme/Opaque.java",
            "package com.acme;\n\nimport org.lib.External;\n\nclass Opaque extends External {\n  int run() { return helper() + baseOp(); }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    let child = "src/com/acme/Child.java#Child";
    assert_eq!(
        callees(root, &format!("{child}.run")),
        [
            to("src/com/acme/Base.java#Base.baseOp", "inherited"),
            to(&format!("{child}.go"), "local"),
        ]
    );
    assert_eq!(
        callees(root, &format!("{child}.Inner.in")),
        [
            to("src/com/acme/Base.java#Base.baseOp", "inherited"),
            to(&format!("{child}.go"), "local"),
        ]
    );
    // An unresolved superclass may declare any name.
    assert_eq!(callees(root, "src/com/acme/Opaque.java#Opaque.run"), []);
}

/// A superclass method wins over an interface default ("class wins");
/// otherwise a default method, also through a superinterface, is inherited.
#[test]
fn interface_default_methods_are_inherited_after_the_superclass_chain() {
    let temp = fixture(&[
        BASE,
        SHAPE,
        NAMED,
        (
            "src/com/acme/Square.java",
            "package com.acme;\n\nclass Square implements Shape {\n  int a() { return area() + this.area() + label().length(); }\n}\n\nclass Tile extends Base implements Shape {\n  int a() { return area(); }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        callees(root, "src/com/acme/Square.java#Square.a"),
        [
            to("src/com/acme/Named.java#Named.label", "inherited"),
            to("src/com/acme/Shape.java#Shape.area", "inherited"),
        ]
    );
    assert_eq!(
        callees(root, "src/com/acme/Square.java#Tile.a"),
        [to("src/com/acme/Base.java#Base.area", "inherited")]
    );
}

#[test]
fn this_and_super_bind_the_enclosing_class() {
    let temp = fixture(&[
        BASE,
        (
            "src/com/acme/Child.java",
            "package com.acme;\n\nclass Child extends Base {\n  @Override public int over() { return super.over() + this.baseOp() + this.over(); }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    let child = "src/com/acme/Child.java#Child";
    assert_eq!(
        callees(root, &format!("{child}.over")),
        [
            to("src/com/acme/Base.java#Base.baseOp", "inherited"),
            to("src/com/acme/Base.java#Base.over", "inherited"),
            to(&format!("{child}.over"), "local"),
        ]
    );
}

/// `import static C.m` and `import static C.*` bring methods in; a plain
/// on-demand import brings in types only; a member shadows a static import.
#[test]
fn static_imports_bind_methods_and_type_imports_do_not() {
    let temp = fixture(&[
        (
            "src/com/acme/util/Strings.java",
            "package com.acme.util;\n\npublic class Strings {\n  public static int trim(int x) { return x; }\n  public static int pad(int x) { return x; }\n}\n",
        ),
        (
            "src/com/acme/util/Maths.java",
            "package com.acme.util;\n\npublic class Maths {\n  public static int square(int x) { return x * x; }\n}\n",
        ),
        (
            "src/com/acme/A.java",
            "package com.acme;\n\nimport static com.acme.util.Strings.trim;\nimport static com.acme.util.Maths.*;\n\nclass A {\n  int a() { return trim(1) + square(2); }\n}\n",
        ),
        (
            "src/com/acme/B.java",
            "package com.acme;\n\nimport com.acme.util.*;\n\nclass B {\n  int b() { return pad(1) + Strings.trim(2); }\n}\n",
        ),
        (
            "src/com/acme/C.java",
            "package com.acme;\n\nimport static com.acme.util.Strings.trim;\n\nclass C {\n  int c() { return trim(1); }\n  int trim(int x) { return x; }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        callees(root, "src/com/acme/A.java#A.a"),
        [
            to("src/com/acme/util/Maths.java#Maths.square", "import-glob"),
            to("src/com/acme/util/Strings.java#Strings.trim", "import"),
        ]
    );
    assert_eq!(
        callees(root, "src/com/acme/B.java#B.b"),
        [to(
            "src/com/acme/util/Strings.java#Strings.trim",
            "type-qualified"
        )]
    );
    assert_eq!(
        callees(root, "src/com/acme/C.java#C.c"),
        [to("src/com/acme/C.java#C.trim", "local")]
    );
}

/// A single-type import shadows a same-package class of that name.
#[test]
fn single_type_imports_shadow_the_package() {
    let temp = fixture(&[
        (
            "src/com/acme/Helper.java",
            "package com.acme;\n\nclass Helper {\n  int help() { return 1; }\n}\n",
        ),
        (
            "src/com/other/Helper.java",
            "package com.other;\n\npublic class Helper {\n  public int help() { return 2; }\n}\n",
        ),
        (
            "src/com/acme/Use.java",
            "package com.acme;\n\nimport com.other.Helper;\n\nclass Use {\n  private final Helper helper = new Helper();\n  int u() { return helper.help() + new Helper().help(); }\n}\n",
        ),
        (
            "src/com/acme/Near.java",
            "package com.acme;\n\nclass Near {\n  int n() { return new Helper().help(); }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    let other = "src/com/other/Helper.java#Helper";
    let ids = |caller: &str| {
        callees(root, caller)
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids("src/com/acme/Use.java#Use"), [other]);
    assert_eq!(
        ids("src/com/acme/Use.java#Use.u"),
        [other.to_owned(), format!("{other}.help")]
    );
    let near = "src/com/acme/Helper.java#Helper";
    assert_eq!(
        ids("src/com/acme/Near.java#Near.n"),
        [near.to_owned(), format!("{near}.help")]
    );
}

/// `T.CONST.m()` calls a method of the constant's type, not of `T`; a
/// nested type path binds the nested type.
#[test]
fn a_field_of_a_type_is_not_the_type() {
    let temp = fixture(&[
        (
            "src/com/acme/Config.java",
            "package com.acme;\n\nclass Config {\n  static final Loader LOADER = null;\n  int load() { return 1; }\n  static class Parser { static int parse() { return 2; } }\n}\n",
        ),
        (
            "src/com/acme/Use.java",
            "package com.acme;\n\nclass Use {\n  int u() { return Config.LOADER.load() + Config.Parser.parse(); }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        callees(root, "src/com/acme/Use.java#Use.u"),
        [to(
            "src/com/acme/Config.java#Config.Parser.parse",
            "type-qualified"
        )]
    );
}

/// Overloads of the one class link together; enum and record members bind
/// through their type.
#[test]
fn overloads_enums_and_records_bind_their_declaring_type() {
    let temp = fixture(&[
        (
            "src/com/acme/Color.java",
            "package com.acme;\n\nenum Color {\n  RED;\n  int code() { return 1; }\n  static Color of(int x) { return RED; }\n}\n",
        ),
        (
            "src/com/acme/Point.java",
            "package com.acme;\n\nrecord Point(int x, int y) {\n  int sum() { return x + y; }\n}\n",
        ),
        (
            "src/com/acme/Use.java",
            "package com.acme;\n\nclass Use {\n  int f(int x) { return x; }\n  int f(String s) { return 0; }\n  int u(Point p) { return f(1) + Color.of(1).code() + p.sum(); }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    let calls = callees(root, "src/com/acme/Use.java#Use.u");
    let ids = calls.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "src/com/acme/Color.java#Color.of",
            "src/com/acme/Point.java#Point.sum",
            "src/com/acme/Use.java#Use.f@4",
            "src/com/acme/Use.java#Use.f@5",
        ]
    );
}

/// Calls inside a lambda bind from the enclosing method; a generic
/// receiver binds its raw type; an untyped lambda parameter and a method
/// reference (`Helper::help`) bind nothing.
#[test]
fn lambdas_generics_and_method_references() {
    let temp = fixture(&[
        (
            "src/com/acme/Box.java",
            "package com.acme;\n\nclass Box<T> {\n  T get() { return null; }\n}\n",
        ),
        (
            "src/com/acme/Helper.java",
            "package com.acme;\n\nclass Helper {\n  int help() { return 1; }\n}\n",
        ),
        (
            "src/com/acme/Use.java",
            "package com.acme;\n\nimport java.util.List;\nimport java.util.function.Function;\n\nclass Use {\n  void u(List<Helper> hs, Box<Helper> b) {\n    hs.forEach(h -> h.help());\n    Runnable r = () -> go();\n    b.get().help();\n    Function<Helper, Integer> f = Helper::help;\n  }\n  void go() {}\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        callees(root, "src/com/acme/Use.java#Use.u"),
        [
            to("src/com/acme/Box.java#Box.get", "receiver-type"),
            to("src/com/acme/Use.java#Use.go", "local"),
        ]
    );
}

/// A package spread over two source roots: the only type of the name binds
/// at low confidence; two candidates bind nothing.
#[test]
fn a_package_over_several_source_roots_binds_only_a_unique_type() {
    let temp = fixture(&[
        (
            "src/main/java/com/acme/Cart.java",
            "package com.acme;\n\nclass Cart {\n  static Cart empty() { return null; }\n}\n",
        ),
        (
            "src/main/java/com/acme/Dup.java",
            "package com.acme;\n\nclass Dup {\n  static int d() { return 1; }\n}\n",
        ),
        (
            "src/main/java/com/other/Dup.java",
            "package com.other;\n\nclass Dup {\n  static int d() { return 2; }\n}\n",
        ),
        (
            "src/test/java/com/acme/CartTest.java",
            "package com.acme;\n\nclass CartTest {\n  void t() { Cart.empty(); Dup.d(); }\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "src/test/java/com/acme/CartTest.java#CartTest.t"),
        [edge(
            "src/main/java/com/acme/Cart.java#Cart.empty",
            "type-qualified",
            "low"
        )]
    );
}
