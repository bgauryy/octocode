//! Rust name resolution as rust-analyzer models it: a bare name is the
//! module's (its items, its `use` bindings, globs, the prelude); paths walk
//! modules from `crate`, `self`, `super` or the current module, inline
//! modules included; `x.m()` reads `x`'s type, where an inherent method
//! shadows a trait method; a `dyn Trait` call names the trait's method.
use super::*;

const LIB: (&str, &str) = (
    "src/lib.rs",
    "pub mod shapes;\npub mod util;\npub mod app;\npub use shapes::Circle;\n",
);

const UTIL: (&str, &str) = (
    "src/util.rs",
    "pub fn helper(_: f64) {}\npub fn square(x: f64) -> f64 { x * x }\npub fn far_away() -> u8 { 1 }\n",
);

const SHAPES: (&str, &str) = (
    "src/shapes.rs",
    "use crate::util::*;\npub trait Area {\n    fn area(&self) -> f64;\n    fn describe(&self) -> f64 { self.area() }\n}\npub struct Circle { r: f64 }\nimpl Circle {\n    pub fn new(r: f64) -> Self { Self::validate(r); Circle { r } }\n    fn validate(r: f64) { helper(r); }\n    pub fn grow(&mut self) { self.scale(2.0); }\n    fn scale(&mut self, k: f64) { self.r *= k; }\n}\nimpl Area for Circle {\n    fn area(&self) -> f64 { square(self.r) }\n}\npub struct Square;\nimpl Square {\n    pub fn area(&self) -> f64 { 2.0 }\n}\nimpl Area for Square {\n    fn area(&self) -> f64 { 1.0 }\n}\npub fn dynamic(shape: &dyn Area) -> f64 { shape.area() }\nmod nested {\n    pub fn deep() -> u8 { super::local_fn() }\n    pub mod deeper {\n        pub fn deepest() -> u8 { super::deep() }\n    }\n}\nfn local_fn() -> u8 { 1 }\npub fn call_nested() -> u8 { nested::deep() + self::nested::deeper::deepest() }\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn t() { let c = Circle::new(1.0); c.area(); local_fn(); }\n}\n",
);

/// A name the module neither declares nor imports is no item of a sibling
/// module, at any confidence.
#[test]
fn rust_bare_names_never_bind_an_unimported_item() {
    let temp = fixture(&[
        CARGO_APP,
        LIB,
        UTIL,
        SHAPES,
        ("src/app.rs", "pub fn run() -> u8 { far_away() }\n"),
    ]);
    let root = ingested(&temp);
    assert_eq!(call_edges(root, "src/app.rs#run"), []);
}

/// `impl` members, `Self::`, associated functions and glob-imported
/// functions bind their declarations.
#[test]
fn rust_impl_blocks_self_paths_and_glob_uses_link() {
    let temp = fixture(&[CARGO_APP, LIB, UTIL, SHAPES]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "src/shapes.rs#Circle.new"),
        [edge("src/shapes.rs#Circle.validate", "local", "high")]
    );
    assert_eq!(
        call_edges(root, "src/shapes.rs#Circle.validate"),
        [edge("src/util.rs#helper", "import-glob", "high")]
    );
    assert_eq!(
        call_edges(root, "src/shapes.rs#Circle.grow"),
        [edge("src/shapes.rs#Circle.scale", "local", "high")]
    );
}

/// Inline modules are modules: `nested::f`, `self::a::b::f` and `super::f`
/// walk them from the module that spells the path.
#[test]
fn rust_inline_module_paths_link() {
    let temp = fixture(&[CARGO_APP, LIB, UTIL, SHAPES]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "src/shapes.rs#call_nested"),
        [
            edge("src/shapes.rs#nested.deep", "module-path", "high"),
            edge("src/shapes.rs#nested.deeper.deepest", "module-path", "high"),
        ]
    );
    assert_eq!(
        call_edges(root, "src/shapes.rs#nested.deep"),
        [edge("src/shapes.rs#local_fn", "module-path", "high")]
    );
    assert_eq!(
        call_edges(root, "src/shapes.rs#nested.deeper.deepest"),
        [edge("src/shapes.rs#nested.deep", "module-path", "high")]
    );
}

/// `c.area()` on a `Circle` is its `impl Area for Circle` method; an
/// inherent `area` shadows the trait's (`s` is a unit struct value); a `dyn
/// Area` receiver and a trait's own `self.area()` name the trait method,
/// never one implementation. Each callee is one row, whichever call found it.
#[test]
fn rust_trait_and_inherent_methods_bind_like_method_resolution() {
    let temp = fixture(&[
        CARGO_APP,
        LIB,
        UTIL,
        SHAPES,
        (
            "src/app.rs",
            "use crate::Circle;\nuse crate::shapes::{self, Area, Square};\npub fn run() -> f64 {\n    let c = Circle::new(1.0);\n    let s = Square;\n    c.area() + s.area() + shapes::Circle::new(2.0).area()\n}\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "src/app.rs#run"),
        [
            edge("src/shapes.rs#Circle.area", "receiver-type", "high"),
            edge("src/shapes.rs#Circle.new", "type-qualified", "high"),
            edge("src/shapes.rs#Square.area@18", "receiver-type", "high"),
        ]
    );
    assert_eq!(
        call_edges(root, "src/shapes.rs#dynamic"),
        [edge("src/shapes.rs#Area.area", "receiver-type", "high")]
    );
    assert_eq!(
        call_edges(root, "src/shapes.rs#Area.describe"),
        [edge("src/shapes.rs#Area.area", "local", "high")]
    );
}

/// `#[cfg(test)] mod tests { use super::*; }` sees its parent's items; its
/// calls are test edges.
#[test]
fn rust_cfg_test_modules_link_through_super_glob() {
    let temp = fixture(&[CARGO_APP, LIB, UTIL, SHAPES]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "src/shapes.rs#tests.t"),
        [
            edge("src/shapes.rs#Circle.area", "receiver-type", "high"),
            edge("src/shapes.rs#Circle.new", "type-qualified", "high"),
            edge("src/shapes.rs#local_fn", "local", "high"),
        ]
    );
    let callers = ask_with(root, "callers", Some("src/shapes.rs#local_fn"), |o| {
        o.confidence = Some("low".into());
    });
    assert_eq!(
        result_row(&callers, "src/shapes.rs#tests.t")["test"],
        true,
        "{callers}"
    );
}

/// A provided trait method the type does not override is the trait's.
#[test]
fn rust_default_trait_methods_bind_through_the_types_impl() {
    let temp = fixture(&[
        CARGO_APP,
        LIB,
        UTIL,
        SHAPES,
        (
            "src/app.rs",
            "use crate::shapes::{Area, Square};\npub fn run() -> f64 { let s = Square; s.describe() }\n",
        ),
    ]);
    let root = ingested(&temp);
    assert_eq!(
        call_edges(root, "src/app.rs#run"),
        [edge("src/shapes.rs#Area.describe", "inherited", "medium")]
    );
}
