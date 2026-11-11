//! Rust target gates: a missed file whose uses a `cfg` of another target
//! leaves out is blamed on that gate, with a rerun under a target where it
//! holds, and never on build scripts that did not run.
use super::super::targets::{AnalysisTarget, Predicate, TargetFacts, cfg_predicates};
use super::*;

/// A target-family gate that does not hold for the host these tests run
/// on, and one that does.
fn family_gates() -> (&'static str, &'static str) {
    if cfg!(windows) {
        ("unix", "windows")
    } else {
        ("windows", "unix")
    }
}

/// A package at `root/pkg` with a build script, whose `src/lib.rs`
/// `include!`s its output, declares `write_atomic` and holds `lib`, beside
/// the `files` of `src`.
fn package(lib: &str, files: &[(&str, &str)]) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("root");
    let source = root.join("pkg/src");
    std::fs::create_dir_all(&source).expect("src");
    std::fs::write(root.join("pkg/Cargo.toml"), "[package]\nname = \"p\"\n").expect("manifest");
    std::fs::write(root.join("pkg/build.rs"), "fn main() {}\n").expect("build.rs");
    std::fs::write(
        source.join("lib.rs"),
        format!(
            "include!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\npub fn write_atomic() {{}}\n{lib}"
        ),
    )
    .expect("lib");
    for (name, text) in files {
        std::fs::write(source.join(name), text).expect("source");
    }
    (dir, root)
}

/// The `references` row of `write_atomic` after the server answered
/// `lib.rs` alone, under the query's `rustContext`.
async fn row(root: &Path, rust_context: Option<Value>) -> Value {
    let package = root.join("pkg");
    let lib_path = package.join("src/lib.rs").to_string_lossy().into_owned();
    let policy = crate::tools::test_support::workspace_policy(root);
    let scope = Scope::new(root.to_string_lossy().into_owned(), vec!["*.rs".into()]);
    scope
        .text_files(
            "write_atomic",
            &policy,
            &crate::execution::cancel::NeverCancel,
        )
        .await
        .expect("scan");
    scope.answer([lib_path.clone()]);
    let mut fields = json!({"path": lib_path, "operation": "references",
        "symbolName": "write_atomic", "line": 2});
    if let Some(context) = rust_context {
        fields["rustContext"] = context;
    }
    let q = query(fields);
    let config = configured_rust(
        &q,
        octocode_engine::lsp::config::rust_analyzer_headless_options(),
    );
    let mut row = json!({"operation":"references","matches":[{"path":"pkg/src/lib.rs"}]});
    let package_text = package.to_string_lossy().into_owned();
    super::super::annotate(
        &mut row,
        &q,
        Some("rust"),
        Server {
            anchor: &lib_path,
            root: &package_text,
        },
        &scope,
        crate::tools::lsp_search::receipt::RustAnalysis::of(&config).allowing_build_scripts(true),
        &|| false,
    );
    row
}

fn reruns(row: &Value) -> Vec<String> {
    row["related"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|call| call["tool"] == "lspSearch")
        .filter_map(|call| {
            call["input"]["queries"][0]["rustContext"]["target"]
                .as_str()
                .map(str::to_owned)
        })
        .collect()
}

/// A module declared under a gate of another target is missing from the
/// answer because of that gate: the row names the gate and the file, leads
/// to a rerun under a target the gate holds for, and does not blame the
/// build scripts the package also has.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_module_behind_another_targets_gate_is_blamed_on_the_gate() {
    let (off, _) = family_gates();
    let (_dir, root) = package(
        &format!("#[cfg({off})]\nmod other;\n"),
        &[("other.rs", "pub fn f() { super::write_atomic(); }\n")],
    );
    let row = row(&root, None).await;
    assert_eq!(row["partialReasons"], json!([CFG_REASON]), "{row}");
    assert_eq!(row["coverage"]["textOnlyFiles"], 1, "{row}");
    let warnings = row["warnings"].to_string();
    assert!(
        warnings.contains(&format!("cfg({off})")) && warnings.contains("pkg/src/other.rs"),
        "{row}"
    );
    assert!(!row.to_string().contains("build scripts"), "{row}");
    let reruns = reruns(&row);
    assert_eq!(reruns.len(), 1, "{row}");
    let target = TargetFacts::of_triple(&reruns[0]).expect("a listed target");
    assert_eq!(
        Predicate::Flag(off.into()).holds(target),
        Some(true),
        "the rerun's target compiles the gated file in: {row}"
    );
}

/// A gate on the item that holds every use explains the file as a gate on
/// its module does; a file with one use outside any such gate is not
/// explained, so the build scripts stay a possible cause beside the gate.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gate_explains_a_file_only_when_it_holds_every_use() {
    let (off, _) = family_gates();
    let gated =
        format!("/// Platform write.\n#[cfg({off})]\nfn f() {{\n    crate::write_atomic();\n}}\n");
    let (_dir, root) = package("mod item;\n", &[("item.rs", &gated)]);
    let row_gated = row(&root, None).await;
    assert_eq!(
        row_gated["partialReasons"],
        json!([CFG_REASON]),
        "{row_gated}"
    );

    let mixed = format!("{gated}fn g() {{\n    crate::write_atomic();\n}}\n");
    let (_dir, root) = package("mod item;\n", &[("item.rs", &mixed)]);
    let row_mixed = row(&root, None).await;
    assert_eq!(
        row_mixed["partialReasons"],
        json!([BUILD_SCRIPTS_REASON]),
        "{row_mixed}"
    );
    assert!(reruns(&row_mixed).is_empty(), "{row_mixed}");

    let (_dir, root) = package(
        &format!("mod item;\n#[cfg({off})]\nmod other;\n"),
        &[
            ("item.rs", "fn g() { crate::write_atomic(); }\n"),
            ("other.rs", "pub fn f() { super::write_atomic(); }\n"),
        ],
    );
    let both = row(&root, None).await;
    assert_eq!(
        both["partialReasons"],
        json!([BUILD_SCRIPTS_REASON, CFG_REASON]),
        "{both}"
    );
    assert_eq!(reruns(&both).len(), 1, "{both}");
}

/// A gate that holds for the analyzed target hides nothing: on the host,
/// and under a `rustContext.target` it holds for. An unlisted target's
/// `cfg` is not known, so no gate is judged against it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_gate_that_holds_for_the_analyzed_target_explains_nothing() {
    let (off, on) = family_gates();
    let (_dir, root) = package(
        &format!("#[cfg({on})]\nmod other;\n"),
        &[("other.rs", "pub fn f() { super::write_atomic(); }\n")],
    );
    let host = row(&root, None).await;
    assert_eq!(
        host["partialReasons"],
        json!([BUILD_SCRIPTS_REASON]),
        "{host}"
    );

    let (_dir, root) = package(
        &format!("#[cfg({off})]\nmod other;\n"),
        &[("other.rs", "pub fn f() { super::write_atomic(); }\n")],
    );
    let holding = if off == "windows" {
        "x86_64-pc-windows-msvc"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    let named = row(&root, Some(json!({"target": holding}))).await;
    assert_eq!(
        named["partialReasons"],
        json!([BUILD_SCRIPTS_REASON]),
        "{named}"
    );
    let unlisted = row(&root, Some(json!({"target": "riscv64gc-unknown-none-elf"}))).await;
    assert_eq!(
        unlisted["partialReasons"],
        json!([BUILD_SCRIPTS_REASON]),
        "{unlisted}"
    );
}

/// `cfg` predicates are read from attributes and judged per target; an
/// option the target does not decide leaves the predicate open, and a
/// `cfg_attr` is no gate.
#[test]
fn cfg_predicates_are_judged_against_a_target() {
    let windows = TargetFacts::of_triple("x86_64-pc-windows-msvc").expect("windows");
    let linux = TargetFacts::of_triple("aarch64-unknown-linux-gnu").expect("linux");
    let judged = |attribute: &str| {
        let found = cfg_predicates(attribute);
        assert_eq!(found.len(), 1, "{attribute}");
        (found[0].holds(windows), found[0].holds(linux))
    };
    assert_eq!(judged("#[cfg(windows)]"), (Some(true), Some(false)));
    assert_eq!(judged("#![cfg(unix)]"), (Some(false), Some(true)));
    assert_eq!(
        judged("#[cfg(all(unix, not(target_os = \"macos\")))]"),
        (Some(false), Some(true))
    );
    assert_eq!(
        judged("#[cfg(any(target_arch = \"aarch64\", target_env = \"msvc\"))]"),
        (Some(true), Some(true))
    );
    assert_eq!(
        judged("#[cfg(all(windows, feature = \"x\"))]"),
        (None, Some(false))
    );
    assert_eq!(judged("#[cfg(any(test, unix))]"), (None, Some(true)));
    assert!(cfg_predicates("#[cfg_attr(windows, path = \"w.rs\")]").is_empty());
    assert!(cfg_predicates("let cfg = (windows);").is_empty());
    assert_eq!(
        AnalysisTarget::of(Some("x86_64-pc-windows-msvc")),
        AnalysisTarget::Listed(windows)
    );
    assert_eq!(
        AnalysisTarget::of(Some("no-such-target")),
        AnalysisTarget::Unknown
    );
    assert_eq!(AnalysisTarget::of(None), AnalysisTarget::Host);
}
