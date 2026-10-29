//! Scala call sites, heritage and term-scope facts.

use super::*;

/// `(callee, receiverType, termBinding)` of every call, in source order.
fn scala_calls(source: &str) -> Vec<(String, Option<String>, bool)> {
    facts_json(source, "a.scala")["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .map(|call| {
            (
                call["callee"].as_str().unwrap_or_default().to_owned(),
                call["receiverType"].as_str().map(str::to_owned),
                call["termBinding"].as_bool().unwrap_or(false),
            )
        })
        .collect()
}

fn call(callee: &str, receiver: Option<&str>, term: bool) -> (String, Option<String>, bool) {
    (callee.to_owned(), receiver.map(str::to_owned), term)
}

#[test]
fn scala_generic_calls_drop_their_type_arguments() {
    let source = "trait T { def m[A](a: A): A; def u = m[Int](1); def v = this.m[Int](2) }\n";
    assert_eq!(
        scala_calls(source),
        [call("m", None, false), call("this.m", None, false)]
    );
}

/// A curried application is one call site at the outermost node, named by
/// the innermost function; its argument site holds every group.
#[test]
fn scala_curried_chains_are_one_site() {
    let source = "object O {\n  def f(a: Int)(b: Int) = a\n  def g = f(1)(2)\n  def h(G: Q) = G.tailRecM[A, B](x)(step)\n  def k = new K[A](1)(2)\n}\n";
    let facts = facts_json(source, "a.scala");
    let calls = facts["calls"].as_array().expect("calls");
    let callees = calls
        .iter()
        .map(|call| call["callee"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    // `new K(1)(2)` is one construction with two parameter lists.
    assert_eq!(callees, ["f", "G.tailRecM", "K"], "{facts}");
    let sites = facts["argumentSites"].as_array().expect("argument sites");
    assert_eq!(sites.len(), 3, "{facts}");
    for (site, call) in sites.iter().zip(calls) {
        assert_eq!(site["callId"], call["id"], "{facts}");
        assert_eq!(site["groups"].as_array().map(Vec::len), Some(2), "{facts}");
    }
}

#[test]
fn scala_heritage_names_every_base_without_type_arguments() {
    let source = "class X[F[_]](a: Int) extends FlatMap.AbstractFlatMap[F](a) with Base[F] with Serializable\nobject O extends T\ntrait M[F[_]] extends FlatMap[F] with Applicative[F]\ncase class K(a: Int)\n";
    assert_eq!(
        heritage(source, "a.scala"),
        vec![
            edge("extends", "X", "FlatMap.AbstractFlatMap", 1),
            edge("extends", "X", "Base", 1),
            edge("extends", "X", "Serializable", 1),
            edge("extends", "O", "T", 2),
            edge("extends", "M", "FlatMap", 3),
            edge("extends", "M", "Applicative", 3),
        ]
    );
}

/// An anonymous template that is a declaration's whole value gives that
/// declaration its bases; one nested anywhere else, or a self type, makes
/// the scope opaque.
#[test]
fn scala_anonymous_templates_and_self_types_are_heritage() {
    let source = "object O {\n  implicit def F: Monad[F] = new Monad[F] with Extra { def m = 1 }\n  val v = new Base { def n = 2 }\n  def w = List(new Base { def k = 3 })\n  def plain = new Base\n}\ntrait S { self: Other => }\n";
    assert_eq!(
        heritage(source, "a.scala"),
        vec![
            edge("extends", "F", "Monad", 2),
            edge("extends", "F", "Extra", 2),
            edge("extends", "v", "Base", 3),
            edge("extends", "w", "<anonymous-template>", 4),
            edge("extends", "S", "<self-type>", 7),
        ]
    );
}

#[test]
fn scala_receivers_take_the_declared_type_of_their_term() {
    let source = "trait T[F[_]] extends Monad[F] {\n  implicit def F: Monad[F]\n  def G: FlatMap.Base[F] = ???\n  val plain = 1\n  def a = F.pure(1)\n  def b = G.flatMapIt(1)\n  def c[H[_]](z: Int)(implicit H: Monad[H]) = H.tailRecM(z)(f)\n  def d(monad: Monad[F]) = { def inner = monad.first(1); inner }\n  def e = { val m: Applicative[F] = ???; m.second(1) }\n  def g[K[_]](fa: K[Int]) = fa.third(f)\n  def h(implicit Q: Monad[Q]) = xs.map { Q => Q.fourth(1) }\n  def i(f: Int => Int) = f(1)\n  def j = plain.fifth()\n  def k = Unbound.sixth()\n  def l = (1, 2) match { case (p, _) => p.seventh() }\n}\nclass K(m: Monad[Option]) { def u = m.eighth(1) }\n";
    assert_eq!(
        scala_calls(source),
        [
            call("F.pure", Some("Monad"), false),
            call("G.flatMapIt", Some("FlatMap.Base"), false),
            call("H.tailRecM", Some("Monad"), false),
            call("monad.first", Some("Monad"), false),
            call("m.second", Some("Applicative"), false),
            call("fa.third", None, true),
            call("xs.map", None, false),
            call("Q.fourth", None, true),
            call("f", None, true),
            call("plain.fifth", None, true),
            call("Unbound.sixth", None, false),
            call("p.seventh", None, true),
            call("m.eighth", Some("Monad"), false),
        ]
    );
}

/// A local `val` is visible only after its definition; a nested `def`
/// shadows an outer parameter.
#[test]
fn scala_term_scope_follows_definition_order_and_nesting() {
    let source = "object O {\n  def a(g: Int => Int) = { def g(x: Int) = x; g(1) }\n  def b = { m.early(1); val m: Monad[F] = ???; m.late(2) }\n  object Inner { def run() = 1 }\n  def c = Inner.run()\n}\n";
    assert_eq!(
        scala_calls(source),
        [
            call("g", None, false),
            call("m.early", None, false),
            call("m.late", Some("Monad"), false),
            call("Inner.run", None, false),
        ]
    );
}

/// `(callee, selfTemplate, termBinding)` of every call, in source order.
fn self_calls(source: &str) -> Vec<(String, Option<String>, bool)> {
    facts_json(source, "a.scala")["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .map(|call| {
            (
                call["callee"].as_str().unwrap_or_default().to_owned(),
                call["selfTemplate"].as_str().map(str::to_owned),
                call["termBinding"].as_bool().unwrap_or(false),
            )
        })
        .collect()
}

/// A self alias (`self =>`) names the template that declares it, however
/// deep the call is nested; one on an anonymous class that no declaration
/// holds names nothing.
#[test]
fn scala_self_aliases_name_their_declaring_template() {
    let source = "trait Outer { self =>\n  def foo() = 1\n  class Inner { def call() = self.foo() }\n  def rev = new Outer { inner => def foo() = self.foo(); def bar() = inner.foo() }\n  def w = List(new Outer { me => def baz() = me.foo() })\n  def plain = this.foo()\n}\n";
    let named = |callee: &str, template: Option<&str>, term: bool| {
        (callee.to_owned(), template.map(str::to_owned), term)
    };
    assert_eq!(
        self_calls(source),
        [
            named("self.foo", Some("Outer"), false),
            named("Outer", None, false),
            named("self.foo", Some("Outer"), false),
            named("inner.foo", Some("rev"), false),
            named("List", None, false),
            named("Outer", None, false),
            named("me.foo", None, true),
            named("this.foo", None, false),
        ]
    );
}

/// An import inside a declaration body records that declaration: Scala
/// imports are visible only in their enclosing block.
#[test]
fn scala_imports_record_their_enclosing_declaration() {
    let source = "import a.B\nclass K {\n  import Local._\n  def x() = { import c.D; 1 }\n}\n";
    let facts = facts_json(source, "a.scala");
    let scopes = facts["imports"]
        .as_array()
        .expect("imports")
        .iter()
        .map(|import| {
            let scope = import["scopeId"].as_str().map(|id| {
                facts["declarations"]
                    .as_array()
                    .expect("declarations")
                    .iter()
                    .find(|decl| decl["id"] == id)
                    .map_or("?", |decl| decl["name"].as_str().unwrap_or("?"))
            });
            (import["specifier"].as_str().unwrap_or_default(), scope)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        scopes,
        [("a.B", None), ("Local._", Some("K")), ("c.D", Some("x"))],
        "{facts}"
    );
}
