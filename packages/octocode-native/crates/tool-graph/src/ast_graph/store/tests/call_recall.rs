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
        "com/x/Preconditions.java#Preconditions.checkNotNull@1",
        "com/x/Preconditions.java#Preconditions.checkNotNull@2",
    ] {
        assert_eq!(
            callers(dir.0.as_path(), overload),
            vec![
                "app/com/x/Static.java#Static.run",
                "app/com/x/User.java#User.run",
                "app/com/x/Wild.java#Wild.run",
            ],
            "{overload}"
        );
    }
    for decoy in [
        "com/y/Preconditions.java#Preconditions.checkNotNull@1",
        "com/y/Preconditions.java#Preconditions.checkNotNull@2",
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
        vec!["app/src/io/read.rs#root"]
    );
    assert_eq!(
        callers(root, "src/io/util/mod.rs#pin_ref"),
        vec!["app/src/io/mod.rs#local", "app/src/io/read.rs#parent"]
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
        vec!["app/src/future/mod.rs#run"]
    );
}

/// A member call on a receiver the graph cannot type names its callee but
/// links to no declaration: `callers` cannot list it, so it says how many
/// such sites name the target and leads to them.
#[test]
fn callers_state_the_call_sites_that_only_name_the_target() {
    let dir = git_app(&[
        (
            "lib/command.js",
            "class Command {\n  _parseCommand(args) { return args; }\n}\nmodule.exports = { Command };\n",
        ),
        (
            "lib/run.js",
            "function run(subCommand, args) {\n  return subCommand._parseCommand(args);\n}\nmodule.exports = { run };\n",
        ),
    ]);
    let built = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let target = "app/lib/command.js#Command._parseCommand";
    let named = ask(dir.0.as_path(), "unresolved", None, |o| {
        o.symbol_name = Some("_parseCommand".into());
    });
    let sites = named.value["rows"].as_array().map_or(0, Vec::len);
    assert_eq!(sites, 1, "{}", named.value);
    let out = ask(dir.0.as_path(), "callers", Some(target), |_| {});
    // The note promises the `unresolved` read by callee name; the served row
    // keeps it.
    octocode_runtime_base::test_support::assert_lead_served(
        octocode_runtime_base::id::ToolId::GraphQuery,
        &out.value,
        |input| {
            input["queries"][0]["operation"] == "unresolved"
                && input["queries"][0]["symbolName"] == "_parseCommand"
        },
    );
    assert!(
        out.value["rows"].as_array().is_none_or(Vec::is_empty),
        "{}",
        out.value
    );
    assert!(
        out.value
            .to_string()
            .contains("1 call site names the target but their receiver could not be resolved"),
        "{} {}",
        out.value,
        named.value
    );
}

/// The unresolved-receiver count is stated on answers that list callers too,
/// as a partial answer; a target no site names only is not partial.
#[test]
fn callers_with_rows_and_unresolved_sites_are_partial_with_the_count() {
    let dir = git_app(&[
        (
            "lib/command.js",
            "class Command {\n  _parseCommand(args) { return args; }\n  _other(args) { return this._parseCommand(args); }\n}\nfunction direct(args) { return _helper(args); }\nfunction _helper(args) { return args; }\nmodule.exports = { Command, direct };\n",
        ),
        (
            "lib/run.js",
            "function run(subCommand, args) {\n  subCommand._parseCommand(args);\n  return subCommand._parseCommand(args);\n}\nfunction again(other, args) {\n  return other._parseCommand(args);\n}\nmodule.exports = { run, again };\n",
        ),
        (
            "lib/own.js",
            "const { direct } = require('./command');\nfunction go(args) { return direct(args); }\nmodule.exports = { go };\n",
        ),
    ]);
    let built = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let unresolved = ask(dir.0.as_path(), "unresolved", None, |o| {
        o.symbol_name = Some("_parseCommand".into());
    });
    assert_eq!(
        unresolved.value["rows"].as_array().map_or(0, Vec::len),
        3,
        "{}",
        unresolved.value
    );
    let partial = ask(
        dir.0.as_path(),
        "callers",
        Some("app/lib/command.js#Command._parseCommand"),
        |_| {},
    );
    assert!(
        partial.value["rows"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "a resolved caller lists beside the unresolved count: {}",
        partial.value
    );
    assert_eq!(partial.value["isPartial"], true, "{}", partial.value);
    assert!(
        partial.value["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.iter().any(|r| r == "callSitesUnresolved")),
        "{}",
        partial.value
    );
    assert!(
        partial.value["warnings"].to_string().contains(
            "3 call sites name the target but their receiver could not be resolved; related lists them by name; text search may find more"
        ),
        "{}",
        partial.value
    );
    let complete = ask(
        dir.0.as_path(),
        "callers",
        Some("lib/command.js#direct"),
        |_| {},
    );
    assert!(
        complete.value["rows"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "{}",
        complete.value
    );
    assert!(
        complete.value.get("isPartial").is_none(),
        "{}",
        complete.value
    );
    assert!(
        !complete
            .value
            .to_string()
            .contains("receiver could not be resolved"),
        "{}",
        complete.value
    );
}

/// `witnesses` scopes by `target` the side that makes the calls: a symbol
/// that is only called lists nothing there, and the answer says `to` is the
/// scope that lists the calls to it.
#[test]
fn witnesses_of_a_called_symbol_lead_to_the_destination_scope() {
    let dir = git_app(&[(
        "lib/x.js",
        "function callee() { return 1; }\nfunction caller() { return callee(); }\nmodule.exports = { callee, caller };\n",
    )]);
    let built = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let id = "lib/x.js#callee";
    let by_target = ask(dir.0.as_path(), "witnesses", Some(id), |o| {
        o.edges = vec!["calls".into()];
    });
    assert!(
        by_target.value["rows"].as_array().is_none_or(Vec::is_empty),
        "{}",
        by_target.value
    );
    assert!(
        by_target.value.to_string().contains("pass it as `to`"),
        "{}",
        by_target.value
    );
    let by_destination = ask(dir.0.as_path(), "witnesses", None, |o| {
        o.edges = vec!["calls".into()];
        o.to = Some(id.into());
    });
    assert_eq!(
        by_destination.value["rows"].as_array().map(Vec::len),
        Some(1),
        "{}",
        by_destination.value
    );
}

/// `callees` states the target's own call sites that link no edge though
/// their name is declared in this repository (an unknown receiver, an
/// ambiguous name): the answer is partial with their count, and the related
/// `unresolved` call scoped to the target lists exactly them. A target
/// whose every call links is not partial.
#[test]
fn callees_with_unlinked_in_repo_sites_are_partial_with_the_count() {
    let dir = git_app(&[(
        "lib/run.js",
        "function helper(args) { return args; }\nfunction run(subCommand, args) {\n  helper(args);\n  subCommand.helper(args);\n  return subCommand.helper(args);\n}\nfunction plain(args) { return helper(args); }\nmodule.exports = { run, plain };\n",
    )]);
    let built = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let partial = ask(
        dir.0.as_path(),
        "callees",
        Some("app/lib/run.js#run"),
        |_| {},
    );
    assert_eq!(
        ids(&partial),
        ["app/lib/run.js#helper"],
        "{}",
        partial.value
    );
    assert_eq!(partial.value["isPartial"], true, "{}", partial.value);
    let warnings = partial.value["warnings"].to_string();
    assert!(warnings.contains("2 call sites"), "{}", partial.value);
    let lead = partial.value["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| call["input"]["queries"][0]["operation"] == "unresolved")
        .unwrap_or_else(|| panic!("an unresolved lead: {}", partial.value));
    let row = lead["input"]["queries"][0].clone();
    let listed = ask(
        dir.0.as_path(),
        "unresolved",
        Some("app/lib/run.js#run"),
        |o| {
            o.reasons = row["reasons"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|reason| reason.as_str().map(str::to_owned))
                .collect();
        },
    );
    assert_eq!(
        listed.value["rows"].as_array().map_or(0, Vec::len),
        2,
        "{}",
        listed.value
    );
    let complete = ask(dir.0.as_path(), "callees", Some("lib/run.js#plain"), |_| {});
    assert!(
        complete.value.get("isPartial").is_none(),
        "{}",
        complete.value
    );
}
