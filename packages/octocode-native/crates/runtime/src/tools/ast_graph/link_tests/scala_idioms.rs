//! Scala idioms: construction, companions, renaming and package-object
//! imports, mixins; a type an unknown wildcard could supply stays medium;
//! implicit and extension dispatch stays unlinked.
use super::*;

const BOX: (&str, &str) = (
    "src/acme/Box.scala",
    "package acme\n\ncase class Box(v: Int)\nobject Box {\n  def make(): Box = new Box(0)\n}\n",
);

/// `new C(..)` constructs the class `C`; `C(..)` applies the companion of
/// a case class (its constructor); `C.m()` binds the object.
#[test]
fn construction_and_companions_bind_the_class_or_object() {
    let temp = fixture(&[
        BOX,
        (
            "src/acme/Use.scala",
            "package acme\n\nobject Use {\n  def a(): Int = new Box(1).v\n  def b(): Int = Box(2).v + Box.make().v\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    let class = "src/acme/Box.scala#Box@3";
    assert_eq!(
        callees(root, "src/acme/Use.scala#Use.a"),
        [to(class, "constructs:same-package")]
    );
    assert_eq!(
        callees(root, "src/acme/Use.scala#Use.b"),
        [
            to("src/acme/Box.scala#Box.make", "type-qualified"),
            to(class, "same-package"),
        ]
    );
    assert_eq!(
        callees(root, "src/acme/Box.scala#Box.make"),
        [to(class, "constructs:local")]
    );
}

/// A renaming selector binds the original member; a wildcard over a
/// package object brings its members in; mixed-in traits are bases.
#[test]
fn renamed_imports_package_objects_and_mixins_link() {
    let temp = fixture(&[
        (
            "src/acme/Base.scala",
            "package acme\n\ntrait Base { def over(): Int = 1 }\ntrait Mixin { def mix(): Int = 3 }\n",
        ),
        (
            "src/acme/util/Strings.scala",
            "package acme.util\n\nobject Strings { def pad(x: Int): Int = x }\n",
        ),
        (
            "src/acme/util/package.scala",
            "package acme\n\npackage object util { def square(x: Int): Int = x * x }\n",
        ),
        (
            "src/acme/Child.scala",
            "package acme\n\nimport acme.util._\nimport acme.util.Strings.{pad => padIt}\n\nclass Child extends Base with Mixin {\n  override def over(): Int = super.over() + mix() + padIt(1) + square(2)\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        callees(root, "src/acme/Child.scala#Child.over"),
        [
            to("src/acme/Base.scala#Base.over", "inherited"),
            to("src/acme/Base.scala#Mixin.mix", "inherited"),
            to("src/acme/util/Strings.scala#Strings.pad", "import"),
            to("src/acme/util/package.scala#square", "import"),
        ]
    );
}

/// A wildcard import over a path outside the scan outranks a package
/// member of another file, so a type it could supply is not certain.
#[test]
fn a_type_an_unknown_wildcard_could_supply_is_medium() {
    let temp = fixture(&[
        (
            "src/cats/Monad.scala",
            "package cats\n\ntrait Monad { def pure(a: Int): Int = a }\n",
        ),
        (
            "src/cats/Known.scala",
            "package cats\n\nobject Known { def k(m: Monad): Int = m.pure(1) }\n",
        ),
        (
            "src/cats/Wild.scala",
            "package cats\n\nimport ext.lib._\n\nobject Wild { def w(m: Monad): Int = m.pure(1) }\n",
        ),
    ]);
    let root = ingested(&temp);
    let pure = "src/cats/Monad.scala#Monad.pure";
    assert_eq!(
        call_edges(root, "src/cats/Known.scala#Known.k"),
        [edge(pure, "receiver-type", "high")]
    );
    assert_eq!(
        call_edges(root, "src/cats/Wild.scala#Wild.w"),
        [edge(pure, "receiver-type", "medium")]
    );
}

/// Implicit resolution, `given` instances and extension methods dispatch
/// on types the syntax does not show: no edge rather than a guess.
#[test]
fn implicit_and_extension_dispatch_stays_unlinked() {
    let temp = fixture(&[
        (
            "src/acme/Show.scala",
            "package acme\n\ntrait Show[A] { def show(a: A): String }\nobject Show {\n  given intShow: Show[Int] with { def show(a: Int): String = a.toString }\n  implicit class Ops(val i: Int) { def shown: String = i.toString }\n  extension (i: Int) def doubled: Int = i * 2\n}\n",
        ),
        (
            "src/acme/Use.scala",
            "package acme\n\nobject Use {\n  def u(): String = summon[Show[Int]].show(1) + implicitly[Show[Int]].show(2) + 3.doubled.toString + 4.shown\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(callees(root, "src/acme/Use.scala#Use.u"), []);
}
