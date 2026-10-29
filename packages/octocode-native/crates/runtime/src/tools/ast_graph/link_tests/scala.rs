//! Scala: unqualified calls bind lexically, then through the enclosing
//! template's linearization; `X.m` binds the object `X`; a typed term binds
//! its declared type. A scope with an unresolved base before the hit links
//! nothing.
use super::*;

const FLAT_MAP: &str = "package cats\ntrait FlatMap[F[_]] extends Apply[F] {\n  def tailRecM[A, B](a: A)(f: A => F[Either[A, B]]): F[B]\n}\n";
const APPLY: &str = "package cats\ntrait Apply[F[_]] extends Serializable {\n  def map[A, B](fa: F[A])(f: A => B): F[B]\n}\n";
const APPLICATIVE: &str =
    "package cats\ntrait Applicative[F[_]] extends Apply[F] {\n  def pure[A](a: A): F[A]\n}\n";
const MONAD: &str = "package cats\ntrait Monad[F[_]] extends FlatMap[F] with Applicative[F] {\n  def w = tailRecM(())(_ => ???)\n}\n";

fn cats(extra: &[(&'static str, &'static str)]) -> crate::tools::test_support::Fixture {
    let mut files = vec![
        ("cats/FlatMap.scala", FLAT_MAP),
        ("cats/Apply.scala", APPLY),
        ("cats/Applicative.scala", APPLICATIVE),
        ("cats/Monad.scala", MONAD),
    ];
    files.extend_from_slice(extra);
    fixture(&files)
}

#[test]
fn generic_and_curried_calls_link_by_their_function_name() {
    let temp = fixture(&[(
        "a.scala",
        "trait T {\n  def m[A](a: A): A = a\n  def f(a: Int)(b: Int) = a\n  def u = m[Int](1)\n  def g = f(1)(2)\n}\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(callees(root, "a.scala#T.u"), [to("a.scala#T.m", "local")]);
    assert_eq!(callees(root, "a.scala#T.g"), [to("a.scala#T.f", "local")]);
    let stats = ask(root, "stats", None);
    assert_eq!(stats["calls"]["sites"], 2, "{stats}");
}

/// A same-named member of a sibling scope is a different identity. (Two of
/// the five call edges are the `new X { .. }` constructions.)
#[test]
fn unqualified_calls_bind_the_nearest_lexical_scope() {
    let temp = fixture(&[(
        "a.scala",
        "trait X\nobject O {\n  val a = new X { def co = { def consume(i: Int): Int = consume(i); 1 } }\n  val b = new X { def co = { def consume(i: Int): Int = consume(i); 2 } }\n}\nclass A { def run = helper(); def helper() = 1 }\nclass B { def helper() = 2 }\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(
        callees(root, "a.scala#O.a.co.consume"),
        [to("a.scala#O.a.co.consume", "local")]
    );
    assert_eq!(
        callees(root, "a.scala#O.b.co.consume"),
        [to("a.scala#O.b.co.consume", "local")]
    );
    assert_eq!(
        callees(root, "a.scala#A.run"),
        [to("a.scala#A.helper", "local")]
    );
    let stats = ask(root, "stats", None);
    assert_eq!(stats["counts"]["edgesByKind"]["calls"], 5, "{stats}");
}

#[test]
fn implicit_this_climbs_the_linearization() {
    let temp = cats(&[]);
    let root = ingested(&temp);
    assert_eq!(
        callees(root, "cats/Monad.scala#Monad.w"),
        [to("cats/FlatMap.scala#FlatMap.tailRecM", "inherited")]
    );
    let stats = ask(root, "stats", None);
    assert!(
        stats["counts"]["edgesByKind"]["inherits"].as_u64() > Some(0),
        "{stats}"
    );
}

/// `C extends A with B`: `B` comes first in `C`'s linearization, so its
/// override is the static target.
#[test]
fn the_first_declaration_in_linearization_order_wins() {
    let temp = fixture(&[(
        "a.scala",
        "trait A { def m() = 1 }\ntrait B extends A { override def m() = 2 }\ntrait C extends A with B { def u = m() }\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(
        callees(root, "a.scala#C.u"),
        [to("a.scala#B.m", "inherited")]
    );
}

/// An unresolved base before the hit could declare the name: no link, not
/// even to an outer declaration. A known member-less marker (`Serializable`)
/// does not block.
#[test]
fn an_unresolved_base_stops_member_lookup() {
    let temp = fixture(&[(
        "a.scala",
        "trait Local { def m() = 1 }\nobject Outer {\n  def m() = 3\n  trait C extends Local with ExternalTrait { def u = m() }\n  trait D extends Local with Serializable { def u = m() }\n  trait E extends Local { def u = toString() }\n  def toString(i: Int) = i\n}\n",
    )]);
    let root = ingested(&temp);
    assert_eq!(callees(root, "a.scala#Outer.C.u"), []);
    assert_eq!(
        callees(root, "a.scala#Outer.D.u"),
        [to("a.scala#Local.m", "inherited")]
    );
    assert_eq!(callees(root, "a.scala#Outer.E.u"), []);
}

#[test]
fn an_import_brings_in_a_same_file_object_member() {
    let source = "object Outer { def create() = 1 }\nclass K { def k = create() }\n";
    let temp = fixture(&[
        ("a.scala", &*format!("import Outer.create\n{source}")),
        ("b.scala", source),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        callees(root, "a.scala#K.k"),
        [to("a.scala#Outer.create", "import")]
    );
    assert_eq!(callees(root, "b.scala#K.k"), []);
}

/// In term position `X.m` names the object `X`, never the class `X`.
#[test]
fn a_qualified_call_binds_the_companion_object() {
    let temp = fixture(&[
        (
            "cats/data/ContT.scala",
            "package cats.data\nsealed abstract class ContT { def tailRecM(a: Int) = 0 }\nobject ContT { def tailRecM(a: Int) = a }\n",
        ),
        (
            "cats/data/Cont.scala",
            "package cats.data\nobject Cont { def t = ContT.tailRecM(1) }\n",
        ),
        (
            "cats/Eval.scala",
            "package cats\nobject Eval extends EvalFunctions\ntrait EvalFunctions { def now(a: Int) = a }\nobject U { def u = Eval.now(1) }\n",
        ),
    ]);
    let root = ingested(&temp);
    let object_member = ask(root, "find", Some("tailRecM"));
    let id = result_ids(&object_member)
        .into_iter()
        .find(|id| id.starts_with("cats/data/ContT.scala#ContT") && id.contains("@3"))
        .unwrap_or_else(|| panic!("{object_member}"))
        .to_owned();
    assert_eq!(
        callees(root, "cats/data/Cont.scala#Cont.t"),
        [to(&id, "type-qualified")]
    );
    assert_eq!(
        callees(root, "cats/Eval.scala#U.u"),
        [to("cats/Eval.scala#EvalFunctions.now", "inherited")]
    );
}

/// Same-named objects in two build-variant directories stay unlinked, and
/// an explicit import of another package's `X` never binds ours.
#[test]
fn ambiguous_or_foreign_objects_stay_unlinked() {
    let temp = fixture(&[
        ("scala-2.12/Dup.scala", "object Dup { def x() = 1 }\n"),
        ("scala-2.13/Dup.scala", "object Dup { def x() = 2 }\n"),
        ("main/Use.scala", "object Use { def u = Dup.x() }\n"),
        (
            "cats/data/Box.scala",
            "package cats.data\nobject Box { def y() = 1 }\n",
        ),
        (
            "app/Foreign.scala",
            "import other.lib.Box\nobject Foreign { def f = Box.y() }\n",
        ),
        (
            "app/Mine.scala",
            "import cats.data.Box\nobject Mine { def f = Box.y() }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(callees(root, "main/Use.scala#Use.u"), []);
    assert_eq!(callees(root, "app/Foreign.scala#Foreign.f"), []);
    assert_eq!(
        callees(root, "app/Mine.scala#Mine.f"),
        [to("cats/data/Box.scala#Box.y", "type-qualified")]
    );
}

#[test]
fn typed_terms_bind_their_declared_type() {
    let temp = cats(&[(
        "cats/Use.scala",
        "package cats\nobject Use {\n  def foldM[G[_]](z: Int)(implicit G: Monad[G]) = G.tailRecM(z)(f)\n  def shadow(implicit G: Monad[G]) = xs.map { G => G.tailRecM(1)(f) }\n  def abstractHead[F[_]](fa: F[Int]) = fa.map(f)\n}\ntrait OptionTMonad[F[_]] extends Monad[F] {\n  implicit def F: Monad[F]\n  def tailRecM[A, B](a: A)(f: A => F[Either[A, B]]): F[B] = F.tailRecM(a)(f)\n  def u = F.pure(1)\n}\n",
    )]);
    let root = ingested(&temp);
    let flat_map = to("cats/FlatMap.scala#FlatMap.tailRecM", "inherited");
    assert_eq!(
        callees(root, "cats/Use.scala#Use.foldM"),
        std::slice::from_ref(&flat_map)
    );
    assert_eq!(callees(root, "cats/Use.scala#Use.shadow"), []);
    assert_eq!(callees(root, "cats/Use.scala#Use.abstractHead"), []);
    assert_eq!(
        callees(root, "cats/Use.scala#OptionTMonad.tailRecM"),
        [flat_map]
    );
    assert_eq!(
        callees(root, "cats/Use.scala#OptionTMonad.u"),
        [to("cats/Applicative.scala#Applicative.pure", "inherited")]
    );
}

/// A member of another file's object is visible only through an import; a
/// package's top-level name through its explicit import.
#[test]
fn bare_calls_reach_other_files_only_through_imports() {
    let temp = fixture(&[
        (
            "cats/data/Ior.scala",
            "package cats.data\nobject Ior { final case class Left(a: Int); def left(a: Int) = a }\n",
        ),
        (
            "cats/data/Use.scala",
            "package cats.data\nimport cats.data.Ior.left\nobject Use { def u = Left(1); def v = left(2) }\n",
        ),
        (
            "cats/Now.scala",
            "package cats\nfinal case class Now(a: Int)\n",
        ),
        (
            "cats/data/T.scala",
            "package cats.data\nimport cats.Now\nobject T { def t = Now(1) }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(callees(root, "cats/data/Use.scala#Use.u"), []);
    assert_eq!(
        callees(root, "cats/data/Use.scala#Use.v"),
        [to("cats/data/Ior.scala#Ior.left", "import")]
    );
    assert_eq!(
        callees(root, "cats/data/T.scala#T.t"),
        [to("cats/Now.scala#Now", "import")]
    );
}
