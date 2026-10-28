use super::*;

fn call_ids(dir: &Path, target: &str, confidence: &str) -> GraphOutput {
    ask(dir, "callees", Some(target), |o| {
        o.confidence = Some(confidence.into());
    })
}

#[test]
fn inherited_calls_follow_bound_bases_and_overrides() {
    let dir = git_app(&[
        (
            "base.ts",
            "export class Base { transaction() { return 1; } }\n",
        ),
        (
            "middle.ts",
            "import { Base as Parent } from './base'; export class Middle extends Parent {}\n",
        ),
        (
            "child.ts",
            "import { Middle } from './middle'; export class Child extends Middle { send() { return this.transaction(); } } class Decoy { transaction() { return 2; } } export function typed(child: Child) { return child.transaction(); }\n",
        ),
        (
            "override.ts",
            "import { Base } from './base'; export class Override extends Base { transaction() { return 3; } send() { return this.transaction(); } inherited() { return super.transaction(); } }\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    for target in [
        "child.ts#Child.send",
        "child.ts#typed",
        "override.ts#Override.inherited",
    ] {
        let out = call_ids(dir.0.as_path(), target, "high");
        assert_eq!(
            ids(&out),
            vec!["base.ts#Base.transaction"],
            "{target}: {}",
            out.value
        );
        assert_eq!(out.value["results"][0]["via"], "inherited");
    }
    let own = call_ids(dir.0.as_path(), "override.ts#Override.send", "high");
    assert_eq!(
        ids(&own),
        vec!["override.ts#Override.transaction"],
        "{}",
        own.value
    );
}

#[test]
fn inherited_ambiguity_is_not_high_confidence_and_cycles_terminate() {
    let dir = git_app(&[
        (
            "a.ts",
            "export class Base { transaction() { return 1; } }\n",
        ),
        (
            "b.ts",
            "export class Base { transaction() { return 2; } }\n",
        ),
        ("barrel.ts", "export * from './a'; export * from './b';\n"),
        (
            "child.ts",
            "import { Base } from './barrel'; export class Child extends Base { send() { return this.transaction(); } }\n",
        ),
        (
            "cycle.ts",
            "class A extends B { run() { return this.missing(); } } class B extends A {} class Decoy { missing() { return 3; } }\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let high = call_ids(dir.0.as_path(), "child.ts#Child.send", "high");
    assert!(ids(&high).is_empty(), "{}", high.value);
    let medium = call_ids(dir.0.as_path(), "child.ts#Child.send", "medium");
    let mut got = ids(&medium);
    got.sort();
    assert_eq!(
        got,
        vec!["a.ts#Base.transaction", "b.ts#Base.transaction"],
        "{}",
        medium.value
    );
    let cycle = call_ids(dir.0.as_path(), "cycle.ts#A.run", "low");
    assert!(
        ids(&cycle).is_empty(),
        "unbound decoy must not fill a cyclic hierarchy: {}",
        cycle.value
    );
}

#[test]
fn implements_is_not_inherited_implementation() {
    let dir = git_app(&[
        (
            "a.ts",
            "interface Contract { transaction(): number; } export class Child implements Contract { send() { return this.transaction(); } }\n",
        ),
        (
            "other.ts",
            "export class Decoy { transaction() { return 1; } }\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = call_ids(dir.0.as_path(), "a.ts#Child.send", "low");
    assert!(ids(&out).is_empty(), "{}", out.value);
}

#[test]
fn rust_glob_calls_preserve_identity_through_reexports_and_cycles() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        (
            "src/lib.rs",
            "mod implementation; mod facade; mod looped; mod api;\n",
        ),
        ("src/implementation.rs", "pub fn route() -> u8 { 1 }\n"),
        (
            "src/facade.rs",
            "pub use crate::implementation::route as chosen; pub use crate::looped::*;\n",
        ),
        ("src/looped.rs", "pub use crate::facade::*;\n"),
        (
            "src/api.rs",
            "use crate::facade::*; pub fn run() -> u8 { chosen() }\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = call_ids(dir.0.as_path(), "src/api.rs#run", "high");
    assert_eq!(
        ids(&out),
        vec!["src/implementation.rs#route"],
        "{}",
        out.value
    );
    assert_eq!(out.value["results"][0]["via"], "import-glob");
}

#[test]
fn rust_glob_ambiguity_and_explicit_shadowing_are_distinct() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        (
            "src/lib.rs",
            "mod a; mod b; mod explicit; mod ambiguous; mod parent;\n",
        ),
        ("src/a.rs", "pub fn route() -> u8 { 1 }\n"),
        ("src/b.rs", "pub fn route() -> u8 { 2 }\n"),
        (
            "src/explicit.rs",
            "use crate::a::route; use crate::b::*; pub fn run() -> u8 { route() }\n",
        ),
        (
            "src/ambiguous.rs",
            "use crate::a::*; use crate::b::*; pub fn run() -> u8 { route() }\n",
        ),
        ("src/parent.rs", "mod child; fn hidden() -> u8 { 3 }\n"),
        (
            "src/parent/child.rs",
            "use super::*; pub fn run() -> u8 { hidden() }\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let explicit = call_ids(dir.0.as_path(), "src/explicit.rs#run", "high");
    assert_eq!(ids(&explicit), vec!["src/a.rs#route"], "{}", explicit.value);
    let high = call_ids(dir.0.as_path(), "src/ambiguous.rs#run", "high");
    assert!(ids(&high).is_empty(), "{}", high.value);
    let medium = call_ids(dir.0.as_path(), "src/ambiguous.rs#run", "medium");
    let mut got = ids(&medium);
    got.sort();
    assert_eq!(
        got,
        vec!["src/a.rs#route", "src/b.rs#route"],
        "{}",
        medium.value
    );
    let hidden = call_ids(dir.0.as_path(), "src/parent/child.rs#run", "high");
    assert_eq!(
        ids(&hidden),
        vec!["src/parent.rs#hidden"],
        "{}",
        hidden.value
    );
}

#[test]
fn rust_glob_unknown_branches_cannot_claim_unique_identity() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        (
            "src/lib.rs",
            "mod known; mod facade; mod api; mod explicit;\n",
        ),
        ("src/known.rs", "pub fn route() -> u8 { 1 }\n"),
        (
            "src/facade.rs",
            "pub use crate::known::*; pub use foreign::*;\n",
        ),
        (
            "src/api.rs",
            "use crate::facade::*; pub fn run() -> u8 { route() }\n",
        ),
        (
            "src/explicit.rs",
            "use crate::known::route; use foreign::*; pub fn run() -> u8 { route() }\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let high = call_ids(dir.0.as_path(), "src/api.rs#run", "high");
    assert!(
        ids(&high).is_empty(),
        "unobserved wildcard branch: {}",
        high.value
    );
    let medium = call_ids(dir.0.as_path(), "src/api.rs#run", "medium");
    assert_eq!(ids(&medium), vec!["src/known.rs#route"], "{}", medium.value);
    let explicit = call_ids(dir.0.as_path(), "src/explicit.rs#run", "high");
    assert_eq!(
        ids(&explicit),
        vec!["src/known.rs#route"],
        "{}",
        explicit.value
    );
}

#[test]
fn unresolved_base_cannot_claim_unique_inherited_implementation() {
    let dir = git_app(&[(
        "app.py",
        "from foreign import Missing\nclass Known:\n    def route(self):\n        return 1\nclass Child(Missing, Known):\n    def send(self):\n        return self.route()\nclass Own(Missing, Known):\n    def route(self):\n        return 2\n    def send(self):\n        return self.route()\n",
    )]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let high = call_ids(dir.0.as_path(), "app.py#Child.send", "high");
    assert!(
        ids(&high).is_empty(),
        "unobserved base branch: {}",
        high.value
    );
    let medium = call_ids(dir.0.as_path(), "app.py#Child.send", "medium");
    assert_eq!(ids(&medium), vec!["app.py#Known.route"], "{}", medium.value);
    let own = call_ids(dir.0.as_path(), "app.py#Own.send", "high");
    assert_eq!(ids(&own), vec!["app.py#Own.route"], "{}", own.value);
}
