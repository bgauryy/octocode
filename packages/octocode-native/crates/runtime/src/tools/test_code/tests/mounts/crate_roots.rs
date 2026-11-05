//! The forms `rustc --emit=dep-info` proves test-only that no parent next to the
//! file names: a `#[path]` reaching in through `..`, a path only a `cfg_attr` under
//! test gives, and a crate root Cargo finds by its manifest or layout. Each
//! fixture was run through rustc with and without `--test` (see
//! `.octocode/octocode-dev/release-check-2026-11-05/eval/graph-fix/rustc_oracle.py`).
use super::*;

const MANIFEST: &str = "[package]\nname = \"tc\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

fn index(root: &Fixture) -> TestCodeIndex {
    TestCodeIndex::new(&workspace_policy(&root.0))
}

/// `#[path = "../x.rs"]` from the crate root and from a module: no directory
/// above the file holds its parent.
#[test]
fn a_path_reaching_in_through_parent_directories_mounts_under_test() {
    let root = Fixture::new();
    root.write("Cargo.toml", MANIFEST);
    root.write(
        "src/lib.rs",
        "pub mod a;\n#[cfg(test)]\n#[path = \"../extra/from_root.rs\"]\nmod from_root;\n#[path = \"../extra/kept.rs\"]\npub mod kept;\n",
    );
    root.write(
        "src/a.rs",
        "#[cfg(test)]\n#[path = \"../extra/from_module.rs\"]\nmod from_module;\n",
    );
    let from_root = root.write("extra/from_root.rs", "pub fn t() {}\n");
    let from_module = root.write("extra/from_module.rs", "pub fn t() {}\n");
    let kept = root.write("extra/kept.rs", "pub fn p() {}\n");
    let index = index(&root);
    assert!(index.test_path(&from_root));
    assert!(index.test_path(&from_module));
    assert!(!index.test_path(&kept));
}

/// A test target reaching out mounts its helper under test; one a plain build
/// also reaches stays production.
#[test]
fn a_test_target_reaching_out_is_the_only_mount_or_not() {
    let root = Fixture::new();
    root.write("Cargo.toml", MANIFEST);
    root.write("src/lib.rs", "pub fn p() {}\n");
    root.write(
        "tests/it.rs",
        "#[path = \"../extra/helper.rs\"]\nmod helper;\n#[path = \"../extra/shared.rs\"]\nmod shared;\n",
    );
    root.write(
        "benches/run.rs",
        "#[path = \"../extra/shared.rs\"]\nmod shared;\nfn main() {}\n",
    );
    let helper = root.write("extra/helper.rs", "pub fn t() {}\n");
    let shared = root.write("extra/shared.rs", "pub fn p() {}\n");
    let index = index(&root);
    assert!(index.test_path(&helper));
    assert!(!index.test_path(&shared));
}

/// What the parent gives only under test is that build's file, and the file its
/// name means otherwise stays production.
#[test]
fn a_cfg_attr_path_under_test_mounts_that_file_only() {
    let root = Fixture::new();
    root.write("Cargo.toml", MANIFEST);
    root.write(
        "src/lib.rs",
        "#[cfg_attr(test, path = \"real.rs\")]\npub mod plain;\n#[cfg_attr(unix, path = \"unixy.rs\")]\npub mod other;\n#[cfg_attr(all(test, unix), allow(dead_code), path = \"both.rs\")]\npub mod both;\n",
    );
    let real = root.write("src/real.rs", "pub fn t() {}\n");
    let plain = root.write("src/plain.rs", "pub fn p() {}\n");
    let unixy = root.write("src/unixy.rs", "pub fn p() {}\n");
    let both = root.write("src/both.rs", "pub fn t() {}\n");
    let index = index(&root);
    assert!(index.test_path(&real));
    assert!(index.test_path(&both));
    assert!(!index.test_path(&plain));
    assert!(!index.test_path(&unixy));
}

/// The same form in a parent past the parse limit.
#[test]
fn a_cfg_attr_path_in_a_parent_too_large_to_parse() {
    let root = Fixture::new();
    root.write("Cargo.toml", MANIFEST);
    let padding = "// padding\n".repeat(octocode_engine::signatures::MAX_PARSE_SIZE / 11 + 1);
    root.write(
        "src/lib.rs",
        format!("{padding}#[cfg_attr(test, path = \"real.rs\")]\npub mod plain;\n"),
    );
    let real = root.write("src/real.rs", "pub fn t() {}\n");
    assert!(index(&root).test_path(&real));
}

/// `autobins = false` leaves `src/bin/gated.rs` no target of its own, so it is
/// what `tool.rs` mounts under test, beside `tool.rs`, not under `tool/`.
#[test]
fn a_binary_in_src_bin_is_a_crate_root_for_its_modules() {
    let root = Fixture::new();
    root.write(
        "Cargo.toml",
        format!(
            "{MANIFEST}autobins = false\n\n[[bin]]\nname = \"tool\"\npath = \"src/bin/tool.rs\"\n"
        ),
    );
    root.write(
        "src/bin/tool.rs",
        "#[cfg(test)]\nmod gated;\nmod always;\nfn main() {}\n",
    );
    let gated = root.write("src/bin/gated.rs", "pub fn t() {}\n");
    let always = root.write("src/bin/always.rs", "pub fn p() {}\n");
    let index = index(&root);
    assert!(index.test_path(&gated));
    assert!(index.mounted_by_name(&gated));
    assert!(!index.test_path(&always));
}

/// With Cargo's default discovery, `src/bin/own.rs` is a binary of its own: its
/// `cargo build` compiles it whatever another binary gates, so it is production.
#[test]
fn a_discovered_binary_stays_production_whatever_another_binary_gates() {
    let root = Fixture::new();
    root.write("Cargo.toml", MANIFEST);
    root.write("src/bin/tool.rs", "#[cfg(test)]\nmod own;\nfn main() {}\n");
    let own = root.write("src/bin/own.rs", "fn main() {}\n");
    assert!(!index(&root).test_path(&own));
}

/// A target's `path` is its crate root: `[[bin]]` and `[lib]` mount what they
/// gate beside themselves, and a `[[test]]` target is test code with its modules.
#[test]
fn a_manifest_path_names_a_crate_root() {
    let root = Fixture::new();
    root.write(
        "Cargo.toml",
        format!(
            "{MANIFEST}\n[lib]\npath = \"code/entry.rs\"\n\n[[bin]]\nname = \"custom\"\npath = \"mylib/root.rs\"\n\n[[test]]\nname = \"it\"\npath = \"checks/it.rs\"\n"
        ),
    );
    root.write(
        "mylib/root.rs",
        "#[cfg(test)]\nmod gated;\nmod kept;\nfn main() {}\n",
    );
    let gated = root.write("mylib/gated.rs", "pub fn t() {}\n");
    let kept = root.write("mylib/kept.rs", "pub fn p() {}\n");
    root.write("code/entry.rs", "#[cfg(test)]\nmod gated_lib;\n");
    let gated_lib = root.write("code/gated_lib.rs", "pub fn t() {}\n");
    let it = root.write("checks/it.rs", "mod helper;\n");
    let helper = root.write("checks/helper.rs", "pub fn t() {}\n");
    let index = index(&root);
    assert!(index.test_path(&gated));
    assert!(!index.test_path(&kept));
    assert!(!index.test_path(&root.0.join("mylib/root.rs")));
    assert!(index.test_path(&gated_lib));
    assert!(index.test_path(&it));
    assert!(index.test_path(&helper));
}

/// A file a parent names by its module name never pays for the crate-wide look:
/// the far directory stays unread, whatever verdict is asked.
#[test]
fn a_file_with_a_named_parent_reads_no_other_directory() {
    let root = Fixture::new();
    root.write("Cargo.toml", MANIFEST);
    root.write("src/lib.rs", "pub mod kept;\n");
    let kept = root.write("src/kept.rs", "pub fn p() {}\n");
    let far = root.write("far/away.rs", "#[path = \"x.rs\"] mod y;\n");
    let index = index(&root);
    assert!(!index.test_path(&kept));
    assert!(!index.watched().contains(&far));
}
