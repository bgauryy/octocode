//! Rust method calls bound through the receiver's type as rust-analyzer
//! types it: a type's method set is every `impl` block of its crate, and a
//! receiver's type may come from a declaration, a constructor, or the
//! declared return type of the call that produced it.
use super::*;

const WORKSPACE: (&str, &str) = (
    "Cargo.toml",
    "[workspace]\nmembers = [\"crates/a\", \"crates/b\"]\nresolver = \"2\"\n",
);

fn package(dir: &'static str, name: &str) -> (&'static str, String) {
    (
        dir,
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
    )
}

/// `self.m()` and `Self::m()` in one `impl Policy` reach the methods another
/// module's `impl Policy` declares, in the caller's crate only: another
/// crate's same-named type is a different type.
#[test]
fn rust_self_calls_reach_impl_blocks_split_across_the_crate() {
    let a = package("crates/a/Cargo.toml", "ra");
    let b = package("crates/b/Cargo.toml", "rb");
    let temp = fixture(&[
        WORKSPACE,
        (a.0, &a.1),
        (b.0, &b.1),
        ("crates/a/src/lib.rs", "pub mod policy;\n"),
        (
            "crates/a/src/policy.rs",
            "mod messages;\npub struct Policy;\nimpl Policy {\n    pub fn run(&self) {\n        self.helper();\n        Self::assoc();\n    }\n}\n",
        ),
        (
            "crates/a/src/policy/messages.rs",
            "use super::Policy;\nimpl Policy {\n    pub(super) fn helper(&self) {}\n    pub(super) fn assoc() {}\n}\n",
        ),
        (
            "crates/b/src/lib.rs",
            "pub struct Policy;\nimpl Policy {\n    pub fn helper(&self) {}\n    pub fn assoc() {}\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "crates/a/src/policy.rs#Policy.run"),
        [
            edge(
                "crates/a/src/policy/messages.rs#Policy.assoc",
                "receiver-type",
                "high"
            ),
            edge(
                "crates/a/src/policy/messages.rs#Policy.helper",
                "receiver-type",
                "high"
            ),
        ]
    );
}

/// The declared return types of the repo's functions, `Self` included.
const RESULT: &str = "pub struct Continuation;
impl Continuation {
    pub fn continued() -> Self { Continuation }
    pub fn open() -> Result<Self, String> { Ok(Continuation) }
    pub fn again(&self) -> Continuation { Continuation }
    pub async fn later() -> Self { Continuation }
    pub fn build(&self) {}
    pub fn finish(&self) {}
}
pub struct Other;
impl Other {
    pub fn build(&self) {}
    pub fn finish(&self) {}
}
";

fn continuation(name: &str, via: &str) -> (String, String, String) {
    edge(&format!("src/result.rs#Continuation.{name}"), via, "high")
}

/// `T::f(..).m()` and `x.f()?.m()`: the receiver's type is the callee's
/// declared return type (`Self`, a named type, `Result<Self, _>` through
/// `?`); a `Result` without `?`, or a future without `.await`, is not `T`.
#[test]
fn rust_method_on_a_call_result_binds_through_the_declared_return_type() {
    let temp = fixture(&[
        ("src/lib.rs", "pub mod result;\npub mod paging;\n"),
        ("src/result.rs", RESULT),
        (
            "src/paging.rs",
            "use crate::result::Continuation;
pub fn next(c: &Continuation) -> Result<(), String> {
    Continuation::continued().build();
    Continuation::open()?.finish();
    c.again().build();
    Ok(())
}
pub async fn wrong() {
    Continuation::open().finish();
    Continuation::later().build();
}
",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "src/paging.rs#next"),
        [
            continuation("again", "receiver-type"),
            continuation("build", "receiver-type"),
            continuation("continued", "namespace"),
            continuation("finish", "receiver-type"),
            continuation("open", "namespace"),
        ]
    );
    assert_eq!(
        call_edges(root, "src/paging.rs#wrong"),
        [
            continuation("later", "namespace"),
            continuation("open", "namespace"),
        ]
    );
}

/// A local bound to a call result (`let x = c.again();`, `let x = T::open()?;`)
/// has that call's declared return type.
#[test]
fn rust_local_bound_to_a_call_result_takes_its_return_type() {
    let temp = fixture(&[
        ("src/lib.rs", "pub mod result;\npub mod locals;\n"),
        ("src/result.rs", RESULT),
        (
            "src/locals.rs",
            "use crate::result::Continuation;
pub fn locals(c: &Continuation) -> Result<(), String> {
    let next = c.again();
    next.build();
    let opened = Continuation::open()?;
    opened.finish();
    Ok(())
}
",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "src/locals.rs#locals"),
        [
            continuation("again", "receiver-type"),
            continuation("build", "receiver-type"),
            continuation("finish", "receiver-type"),
            continuation("open", "namespace"),
        ]
    );
}

/// `.unwrap()`/`.expect(..)` on a value whose resolved declared type is std
/// `Result<T, _>`/`Option<T>` yield `T`, as `?` does.
#[test]
fn rust_unwrap_and_expect_on_a_declared_result_yield_its_value_type() {
    let temp = fixture(&[
        ("src/lib.rs", "pub mod result;\npub mod unwrapped;\n"),
        ("src/result.rs", RESULT),
        (
            "src/unwrapped.rs",
            "use crate::result::{Continuation, Other};
pub fn found() -> Option<Other> { None }
pub fn unwrapped() {
    Continuation::open().unwrap().finish();
    let opened = Continuation::open().expect(\"open\");
    opened.build();
    found().unwrap().build();
}
",
        ),
    ]);
    let root = ingested(&temp);
    let edges = call_edges(root, "src/unwrapped.rs#unwrapped");
    for expected in [
        continuation("build", "receiver-type"),
        continuation("finish", "receiver-type"),
        edge("src/result.rs#Other.build", "receiver-type", "high"),
    ] {
        assert!(edges.contains(&expected), "{expected:?} in {edges:?}");
    }
    assert!(
        !edges
            .iter()
            .any(|(id, ..)| id == "src/result.rs#Other.finish"),
        "{edges:?}"
    );
}

/// A crate type named `Result` that declares its own `unwrap` is not std
/// `Result`: its `unwrap` is linked and typed like any method, never by name.
#[test]
fn rust_unwrap_on_a_crate_result_type_follows_its_own_method() {
    let temp = fixture(&[(
        "src/lib.rs",
        "pub struct Wrapped;
impl Wrapped { pub fn finish(&self) {} }
pub struct Result<T>(T);
impl<T> Result<T> { pub fn unwrap(self) -> Wrapped { Wrapped } }
pub struct Continuation;
impl Continuation {
    pub fn open() -> Result<Self> { Result(Continuation) }
    pub fn finish(&self) {}
}
pub fn guarded() {
    Continuation::open().unwrap().finish();
}
",
    )]);
    let root = ingested(&temp);
    let edges = call_edges(root, "src/lib.rs#guarded");
    assert!(
        edges.contains(&edge("src/lib.rs#Wrapped.finish", "receiver-type", "high")),
        "{edges:?}"
    );
    assert!(
        !edges
            .iter()
            .any(|(id, ..)| id == "src/lib.rs#Continuation.finish"),
        "{edges:?}"
    );
}

/// A constructor declared `-> Result<Self, E>`: `T::new(..).expect(..)` and
/// `.unwrap()` yield `T` from that declared type, not from the name `new`.
#[test]
fn rust_unwrap_on_a_fallible_constructor_yields_its_type() {
    let temp = fixture(&[(
        "src/lib.rs",
        "pub struct Q;
impl Q {
    pub fn new() -> Result<Self, String> { Ok(Q) }
    pub fn m(&self) {}
}
pub fn f() {
    Q::new().expect(\"q\").m();
    Q::new().unwrap().m();
}
",
    )]);
    let root = ingested(&temp);
    let edges = call_edges(root, "src/lib.rs#f");
    for expected in [
        edge("src/lib.rs#Q.m", "receiver-type", "high"),
        edge("src/lib.rs#Q.new", "type-qualified", "high"),
    ] {
        assert!(edges.contains(&expected), "{expected:?} in {edges:?}");
    }
}

/// A `static` inside a function: its initializer's `Type::f()` is the
/// function's call, and the static's declared type types `STATIC.m()`. An
/// enum variant constructor (`Attempt::Done(1)`) is not a call.
#[test]
fn rust_static_initializers_and_declared_static_types_bind() {
    let temp = fixture(&[(
        "src/lib.rs",
        "pub struct Pool;
impl Pool {
    pub const fn new() -> Self { Pool }
    pub fn get(&self) {}
}
pub enum Attempt { Done(u8) }
pub fn shared() -> Attempt {
    static POOL: Pool = Pool::new();
    POOL.get();
    Attempt::Done(1)
}
",
    )]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "src/lib.rs#shared"),
        [
            edge("src/lib.rs#Pool.get", "receiver-type", "high"),
            edge("src/lib.rs#Pool.new", "type-qualified", "high"),
        ]
    );
}
