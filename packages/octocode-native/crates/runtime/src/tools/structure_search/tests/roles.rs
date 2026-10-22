//! Listing field roles (contract `x-listing`): every field core gives one
//! reaches the walk and the walk identity a cursor continues; no window
//! field does.

use super::*;
use crate::contracts::listing_roles::structure_search::FIELDS;
use crate::tools::structure_search::listing::Listing;
use octocode_contract_types::{StructureSearchQueryFiles, StructureSearchQueryTree};

/// A value of each listing field that changes what the fixture of
/// [`every_listing_field_changes_what_the_listing_lists`] lists. A role core
/// gives a new field fails both tests here until native applies the field
/// to the walk and this table names a value that shows it.
fn sample(field: &str) -> Value {
    match field {
        "access" => json!("executable"),
        "defaultExcludes" => json!(false),
        "empty" => json!(true),
        "entryType" => json!("d"),
        "exclude" => json!(["sub"]),
        "extensions" => json!(["md"]),
        "include" => json!(["*.rs"]),
        "includeHidden" => json!(true),
        "includeIgnored" => json!(true),
        "maxDepth" => json!(1),
        "minDepth" => json!(2),
        "nameRegex" => json!("^b"),
        "permissions" => json!("600"),
        "size" => json!({"greater": "1k"}),
        "time" => json!({"modifiedBefore": "1d"}),
        other => {
            panic!("core gave {other} a listing role: apply it to the walk, then name a value here")
        }
    }
}

/// The operations whose query variant declares `field`.
fn operations_of(field: &str) -> Vec<&'static str> {
    let contract = crate::contracts::tool_contract(crate::id::ToolId::StructureSearch)
        .expect("structureSearch contract");
    contract["querySchema"]["anyOf"]
        .as_array()
        .expect("query variants")
        .iter()
        .filter(|variant| variant["properties"].get(field).is_some())
        .map(|variant| {
            variant["properties"]["operation"]["const"]
                .as_str()
                .expect("operation")
        })
        .collect()
}

/// Set alone, every listing field changes what the listing lists, in each
/// operation that declares it.
#[cfg(unix)]
#[test]
fn every_listing_field_changes_what_the_listing_lists() {
    use std::os::unix::fs::PermissionsExt;
    let root = Fixture::new();
    let write = |name: &str, text: &str, mode: u32| {
        let path = root.0.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(&path, text).expect("file");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).expect("mode");
    };
    // No repository: the default prune leaves `node_modules` out, and the
    // `.ignore` file hides `ignored.log`.
    write(".ignore", "ignored.log\n", 0o644);
    write("ignored.log", "x\n", 0o644);
    write(".hidden", "x\n", 0o644);
    write("a.rs", "x\n", 0o755);
    write("b.md", "x\n", 0o600);
    write("big.txt", &"x".repeat(2048), 0o644);
    write("empty.txt", "", 0o644);
    write("old.txt", "x\n", 0o644);
    write("sub/deep/c.rs", "x\n", 0o644);
    write("node_modules/pkg/index.js", "x\n", 0o644);
    let month = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 86_400);
    std::fs::File::options()
        .write(true)
        .open(root.0.join("old.txt"))
        .and_then(|file| file.set_modified(month))
        .expect("old mtime");
    let paths = |query: &Value| {
        let out = run(&root.0, query.clone()).unwrap_or_else(|error| panic!("{query}: {error:?}"));
        listed(&out)
            .iter()
            .map(|row| row["path"].as_str().expect("path").to_owned())
            .collect::<Vec<_>>()
    };
    for (field, _) in FIELDS {
        let operations = operations_of(field);
        assert!(!operations.is_empty(), "{field}: no operation declares it");
        for operation in operations {
            // The tree walks three levels, so a depth of one narrows it.
            let mut base = json!({"operation": operation, "path": root.0});
            if operation == "tree" {
                base["maxDepth"] = json!(3);
            }
            let mut set = base.clone();
            set[*field] = sample(field);
            assert_ne!(paths(&base), paths(&set), "{operation}.{field}");
        }
    }
}

/// The walk identity holds every listing field and the operation, and no
/// field that only cuts, orders or briefs the window: a cursor resumes the
/// walk that left it whatever page, page size, ceiling, sort or detail
/// asks for it.
#[test]
fn the_walk_identity_is_the_listing_fields_and_no_window_field() {
    let root = std::path::Path::new("/listed");
    let identity = |operation: &str, changes: Value| {
        let mut row = json!({"operation": operation, "path": "."});
        for (field, value) in changes.as_object().expect("changes") {
            row[field] = value.clone();
        }
        if operation == "tree" {
            serde_json::from_value::<StructureSearchQueryTree>(row)
                .expect("tree")
                .walk_identity(root)
        } else {
            serde_json::from_value::<StructureSearchQueryFiles>(row)
                .expect("files")
                .walk_identity(root)
        }
    };
    assert_ne!(identity("tree", json!({})), identity("files", json!({})));
    for (field, _) in FIELDS {
        for operation in operations_of(field) {
            assert_ne!(
                identity(operation, json!({})),
                identity(operation, json!({*field: sample(field)})),
                "{operation}.{field}"
            );
        }
    }
    for (field, value) in [
        ("page", json!(2)),
        ("pageSize", json!(5)),
        ("snapshot", json!("a".repeat(64))),
        ("scanOffset", json!(3)),
        ("walkOffset", json!(4)),
        ("maxEntries", json!(7)),
        ("extras", json!(["details"])),
        ("mainGoal", json!("goal")),
        ("reasoning", json!("why")),
    ] {
        for operation in ["tree", "files"] {
            assert_eq!(
                identity(operation, json!({})),
                identity(operation, json!({field: value})),
                "{operation}.{field}"
            );
        }
    }
    assert_eq!(
        identity("files", json!({})),
        identity("files", json!({"sort": "size"}))
    );
}
