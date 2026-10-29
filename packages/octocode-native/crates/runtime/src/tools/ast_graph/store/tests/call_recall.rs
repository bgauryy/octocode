//! Call-resolution recall across languages: call forms that must link to
//! the one declaration they name, with decoys that must stay unlinked.
use super::*;

fn callers(dir: &Path, target: &str) -> Vec<String> {
    let out = ask(dir, "callers", Some(target), |o| {
        o.confidence = Some("low".into());
    });
    let mut rows = ids(&out);
    rows.sort();
    rows.dedup();
    rows
}

/// A syntax error in one method leaves its sibling declarations' own
/// extents intact, so they stay call targets; a static call through the
/// type name links every overload of that one type, never a same-named
/// type in another package.
#[test]
fn java_members_beside_a_recovered_sibling_link_static_and_imported_calls() {
    let dir = git_app(&[
        (
            "com/x/Preconditions.java",
            "package com.x;\n\npublic final class Preconditions {\n  public static <T> T checkNotNull(T reference) {\n    return reference;\n  }\n\n  public static <T> T checkNotNull(T reference, Object message) {\n    return reference;\n  }\n\n  static void broken() {\n    int x = ;\n  }\n}\n",
        ),
        (
            "com/x/User.java",
            "package com.x;\n\nfinal class User {\n  static String run(String value) {\n    return Preconditions.checkNotNull(value);\n  }\n}\n",
        ),
        (
            "com/x/Static.java",
            "package com.x;\n\nimport static com.x.Preconditions.checkNotNull;\n\nfinal class Static {\n  static String run(String value) {\n    return checkNotNull(value);\n  }\n}\n",
        ),
        (
            "com/x/Wild.java",
            "package com.x;\n\nimport static com.x.Preconditions.*;\n\nfinal class Wild {\n  static String run(String value) {\n    return checkNotNull(value, \"value\");\n  }\n}\n",
        ),
        (
            "com/y/Preconditions.java",
            "package com.y;\n\npublic final class Preconditions {\n  public static <T> T checkNotNull(T reference) {\n    return reference;\n  }\n\n  public static <T> T checkNotNull(T reference, Object message) {\n    return reference;\n  }\n}\n",
        ),
    ]);
    let built = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    for overload in [
        "com/x/Preconditions.java#Preconditions.checkNotNull@4",
        "com/x/Preconditions.java#Preconditions.checkNotNull@8",
    ] {
        assert_eq!(
            callers(dir.0.as_path(), overload),
            vec![
                "com/x/Static.java#Static.run",
                "com/x/User.java#User.run",
                "com/x/Wild.java#Wild.run",
            ],
            "{overload}"
        );
    }
    for decoy in [
        "com/y/Preconditions.java#Preconditions.checkNotNull@4",
        "com/y/Preconditions.java#Preconditions.checkNotNull@8",
    ] {
        assert!(callers(dir.0.as_path(), decoy).is_empty(), "{decoy}");
    }
}

/// `crate::`, `self::` and `super::` paths name one module file exactly;
/// a same-named module nearer the caller is a decoy.
#[test]
fn rust_module_paths_resolve_by_their_written_segments() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        ("src/lib.rs", "pub mod io;\npub mod util;\n"),
        ("src/util/mod.rs", "pub fn pin_ref(x: u8) -> u8 { x }\n"),
        (
            "src/io/mod.rs",
            "pub mod read;\npub mod util;\npub fn local() -> u8 { self::util::pin_ref(1) }\n",
        ),
        (
            "src/io/util/mod.rs",
            "pub fn pin_ref(x: u8) -> u8 { x + 1 }\n",
        ),
        (
            "src/io/read.rs",
            "pub fn root() -> u8 { crate::util::pin_ref(1) }\npub fn parent() -> u8 { super::util::pin_ref(2) }\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let root = dir.0.as_path();
    assert_eq!(
        callers(root, "src/util/mod.rs#pin_ref"),
        vec!["src/io/read.rs#root"]
    );
    assert_eq!(
        callers(root, "src/io/util/mod.rs#pin_ref"),
        vec!["src/io/mod.rs#local", "src/io/read.rs#parent"]
    );
}

/// A type reached through its module path (`crate::a::b::Type::new()`)
/// links to that type's associated function.
#[test]
fn rust_type_paths_link_associated_functions() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        ("src/lib.rs", "pub mod future;\npub mod runtime;\n"),
        ("src/runtime/mod.rs", "pub mod park;\n"),
        (
            "src/runtime/park.rs",
            "pub struct Parked {}\nimpl Parked {\n    pub fn new() -> Self { Parked {} }\n}\n",
        ),
        (
            "src/future/mod.rs",
            "pub fn run() { let _ = crate::runtime::park::Parked::new(); }\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    assert_eq!(
        callers(dir.0.as_path(), "src/runtime/park.rs#Parked.new"),
        vec!["src/future/mod.rs#run"]
    );
}
