//! Scala scoping: a self alias names the template that declares it, a typed
//! self type hides which member a name reaches, and an import is visible
//! only inside its enclosing declaration.
use super::*;

/// `self.m()` inside a nested class or an anonymous class value reaches
/// the outer template's member, never the inner same-named one.
#[test]
fn a_self_alias_binds_the_template_that_declares_it() {
    let temp = fixture(&[(
        "a.scala",
        "trait Outer { self =>\n  def foo(): Int = 1\n  class Inner {\n    def foo(): Int = 2\n    def call(): Int = self.foo()\n  }\n  def reverse: Outer = new Outer {\n    override def foo(): Int = self.foo()\n  }\n  def own(): Int = self.foo()\n}\n",
    )]);
    let root = ingested(&temp);
    let outer = to("a.scala#Outer.foo", "local");
    assert_eq!(
        callees(root, "a.scala#Outer.Inner.call"),
        std::slice::from_ref(&outer)
    );
    assert_eq!(
        callees(root, "a.scala#Outer.reverse.foo"),
        std::slice::from_ref(&outer)
    );
    assert_eq!(callees(root, "a.scala#Outer.own"), [outer]);
}

/// With `self: T =>` the template's own member may be an overload or
/// override in `T`: a call that finds the name there links nothing.
#[test]
fn a_typed_self_type_hides_the_templates_own_members() {
    let temp = fixture(&[(
        "a.scala",
        "final class Box { def unwrap(): Int = 1 }\ntrait BoxCompat { self: Box =>\n  def unwrap(f: Int): Int = self.unwrap()\n  def bare(): Int = unwrap(1)\n  def viaThis(): Int = this.unwrap(1)\n}\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(callees(root, "a.scala#BoxCompat.unwrap"), []);
    assert_eq!(callees(root, "a.scala#BoxCompat.bare"), []);
    assert_eq!(callees(root, "a.scala#BoxCompat.viaThis"), []);
}

/// An import inside a class is visible only there; a wildcard over a name
/// outside the repo may supply any name, so it never yields to a sibling's
/// import or to a graph-wide match.
#[test]
fn imports_are_scoped_to_their_enclosing_declaration() {
    let temp = fixture(&[
        (
            "b/Scoped.scala",
            "package b\nobject Local { def go(): Int = 2 }\nclass B1 {\n  import Local._\n  def x(): Int = go()\n}\nclass B2 {\n  import ext.Helpers._\n  def y(): Int = go()\n}\n",
        ),
        ("top/Top.scala", "package top\ndef run(): Int = 1\n"),
        (
            "c/Use.scala",
            "package c\nimport ext.tools._\nobject Use { def u(): Int = run() }\n",
        ),
        (
            "c/Plain.scala",
            "package c\nobject Plain { def p(): Int = run(); def q(): Int = Util.go() }\n",
        ),
        (
            "a/Util.scala",
            "package a\nobject Util { def go(): Int = 1 }\n",
        ),
        (
            "c/Wild.scala",
            "package c\nimport ext.tools._\nobject Wild { def w(): Int = Util.go() }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        callees(root, "b/Scoped.scala#B1.x"),
        [to("b/Scoped.scala#Local.go", "import")]
    );
    assert_eq!(callees(root, "b/Scoped.scala#B2.y"), []);
    assert_eq!(callees(root, "c/Use.scala#Use.u"), []);
    assert_eq!(callees(root, "c/Plain.scala#Plain.p").len(), 1);
    assert_eq!(callees(root, "c/Plain.scala#Plain.q").len(), 1);
    assert_eq!(callees(root, "c/Wild.scala#Wild.w"), []);
}
