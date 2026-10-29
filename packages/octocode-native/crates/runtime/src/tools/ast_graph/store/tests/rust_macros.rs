//! Calls read from the arguments of Rust std expression macros.
use super::*;

const CARGO: (&str, &str) = (
    "Cargo.toml",
    "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
);

#[test]
fn a_ready_argument_links_its_module_path_callee() {
    let dir = git_app(&[
        CARGO,
        ("src/lib.rs", "pub mod task;\npub mod a;\n"),
        ("src/task.rs", "pub mod coop;\n"),
        ("src/task/coop.rs", "pub fn poll_proceed(cx: u8) {}\n"),
        (
            "src/a.rs",
            "use std::task::ready;\npub fn x(cx: u8) {\n    ready!(crate::task::coop::poll_proceed(cx));\n}\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(
        dir.0.as_path(),
        "callers",
        Some("src/task/coop.rs#poll_proceed"),
        |_| {},
    );
    let rows = out.value["results"].as_array().expect("rows");
    assert!(
        rows.iter().any(|row| row["id"] == "src/a.rs#x"
            && row["via"] == "module-path"
            && row["confidence"] == "high"),
        "{}",
        out.value
    );
}

#[test]
fn a_pattern_in_matches_is_not_a_call_of_a_tuple_struct() {
    let dir = git_app(&[
        CARGO,
        (
            "src/lib.rs",
            "pub struct Foo(pub u8);\npub fn m(v: Foo) -> bool {\n    matches!(v, Foo(_))\n}\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(dir.0.as_path(), "callers", Some("src/lib.rs#Foo"), |o| {
        o.confidence = Some("low".into());
    });
    assert_eq!(ids(&out), Vec::<String>::new(), "{}", out.value);
}

#[test]
fn a_repo_macro_sharing_a_std_name_leaves_the_argument_unlinked() {
    let dir = git_app(&[
        CARGO,
        ("src/lib.rs", "#[macro_use]\nmod m;\npub mod a;\n"),
        (
            "src/m.rs",
            "macro_rules! ready { ($e:expr) => { stringify!($e) } }\n",
        ),
        (
            "src/a.rs",
            "pub fn helper() {}\npub fn x() {\n    ready!(helper());\n}\npub fn y() {\n    assert!(helper_two());\n}\npub fn helper_two() -> bool { true }\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let root = dir.0.as_path();
    let callers = ask(root, "callers", Some("src/a.rs#helper"), |o| {
        o.confidence = Some("low".into());
    });
    assert!(
        !ids(&callers).contains(&"src/a.rs#x".to_owned()),
        "{}",
        callers.value
    );
    let unresolved = ask(root, "unresolved", Some("src/a.rs#x"), |_| {});
    let rows = unresolved.value["results"].as_array().expect("rows");
    assert!(
        rows.iter()
            .any(|row| row["targetName"] == "helper" && row["reason"] == "ambiguous"),
        "{}",
        unresolved.value
    );
    // An unshadowed std macro in the same graph still links.
    let two = ask(root, "callers", Some("src/a.rs#helper_two"), |_| {});
    assert_eq!(ids(&two), vec!["src/a.rs#y"], "{}", two.value);
}

#[test]
fn a_macro_rules_arm_that_invokes_a_std_macro_does_not_shadow_it() {
    let dir = git_app(&[
        CARGO,
        (
            "src/lib.rs",
            "macro_rules! check { ($e:expr) => {{ assert_eq!($e, 1); }} }\npub fn helper() -> u8 { 1 }\npub fn x() {\n    assert_eq!(helper(), 1);\n    check!(2);\n}\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let out = ask(
        dir.0.as_path(),
        "callers",
        Some("src/lib.rs#helper"),
        |_| {},
    );
    assert_eq!(ids(&out), vec!["src/lib.rs#x"], "{}", out.value);
}

#[test]
fn macro_rules_arms_do_not_create_graph_declarations() {
    let dir = git_app(&[
        CARGO,
        ("src/lib.rs", "#[macro_use]\nmod trace;\npub mod a;\n"),
        (
            "src/trace.rs",
            "macro_rules! trace { ($h:expr) => { if let Some(id) = $h { let _ = id; } } }\n",
        ),
        (
            "src/a.rs",
            "pub fn plain() -> Option<u8> {\n    Some(1)\n}\npub fn asserted() {\n    assert_eq!(Some(1), plain());\n}\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let root = dir.0.as_path();
    let phantom = ask(root, "find", Some("Some"), |_| {});
    assert_eq!(ids(&phantom), Vec::<String>::new(), "{}", phantom.value);
    let plain = ask(root, "callees", Some("src/a.rs#plain"), |o| {
        o.confidence = Some("low".into());
    });
    assert_eq!(ids(&plain), Vec::<String>::new(), "{}", plain.value);
    let asserted = ask(root, "callees", Some("src/a.rs#asserted"), |o| {
        o.confidence = Some("low".into());
    });
    assert_eq!(ids(&asserted), vec!["src/a.rs#plain"], "{}", asserted.value);
}

#[test]
fn a_named_macro_nested_in_a_function_remains_a_call_target() {
    let dir = git_app(&[
        CARGO,
        (
            "src/lib.rs",
            "pub fn use_macro() {\n    macro_rules! local_value { () => { 1 }; }\n    local_value!();\n}\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let callers = ask(dir.0.as_path(), "callers", Some("local_value"), |_| {});
    assert_eq!(
        ids(&callers),
        vec!["src/lib.rs#use_macro"],
        "{}",
        callers.value
    );
}

#[test]
fn a_macro_argument_in_a_recovered_declaration_stays_recovered() {
    let dir = git_app(&[
        CARGO,
        (
            "src/lib.rs",
            "pub fn target() -> u8 { 3 }\npub fn broken(x: u8) -> u8 {\n    let y = match x { 1 => , };\n    assert!(target() == 1);\n    x\n}\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let root = dir.0.as_path();
    let callers = ask(root, "callers", Some("src/lib.rs#target"), |o| {
        o.confidence = Some("low".into());
    });
    assert_eq!(ids(&callers), Vec::<String>::new(), "{}", callers.value);
    let unresolved = ask(root, "unresolved", Some("src/lib.rs"), |_| {});
    let rows = unresolved.value["results"].as_array().expect("rows");
    assert!(
        rows.iter()
            .any(|row| row["targetName"] == "target" && row["reason"] == "recovered-declaration"),
        "{}",
        unresolved.value
    );
}

/// A repo macro imported under a std name (`use my_ready as ready`) expands
/// instead of the std one, in the file that binds it and through a glob.
#[test]
fn a_repo_macro_aliased_to_a_std_name_leaves_the_argument_unlinked() {
    let dir = git_app(&[
        CARGO,
        ("src/lib.rs", "pub mod b;\npub mod c;\n"),
        (
            "src/b.rs",
            "macro_rules! my_ready { ($e:expr) => { stringify!($e) } }\npub(crate) use my_ready as ready;\npub fn aliased() -> u8 { 4 }\npub fn uses_alias() { let _ = ready!(aliased()); }\n",
        ),
        (
            "src/c.rs",
            "use crate::b::*;\npub fn globbed() -> u8 { 5 }\npub fn uses_glob() { let _ = ready!(globbed()); }\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let root = dir.0.as_path();
    for target in ["src/b.rs#aliased", "src/c.rs#globbed"] {
        let callers = ask(root, "callers", Some(target), |o| {
            o.confidence = Some("low".into());
        });
        assert_eq!(ids(&callers), Vec::<String>::new(), "{}", callers.value);
    }
    let unresolved = ask(root, "unresolved", Some("src/c.rs#uses_glob"), |_| {});
    let rows = unresolved.value["results"].as_array().expect("rows");
    assert!(
        rows.iter()
            .any(|row| row["targetName"] == "globbed" && row["reason"] == "ambiguous"),
        "{}",
        unresolved.value
    );
}
