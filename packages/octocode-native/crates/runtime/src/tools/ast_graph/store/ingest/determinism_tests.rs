//! A snapshot's bytes do not depend on how many threads built it: restore,
//! catalog checks and origin encoding run in parallel, and every result
//! joins in scan order.
use super::*;
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::{Fixture, workspace_policy};
use octocode_engine::search::cpu;

/// `(sha256, graph bytes)` of a fresh build of `app/` with every file's
/// facts spilled, so restoring decodes artifacts.
fn build(dir: &Path) -> (String, Vec<u8>) {
    let options = IngestOptions {
        path: dir.join("app"),
        workspace: Some(dir.to_path_buf()),
        refresh: true,
        ..IngestOptions::default()
    };
    let limits = Limits {
        retained_bytes: 0,
        ..LIMITS
    };
    let out = ingest_with_registry(
        &options,
        &workspace_policy(dir),
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        RegistryView::installed(),
        &limits,
    );
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    let graph = PathBuf::from(out.value["dir"].as_str().expect("dir")).join(GRAPH_FILE);
    (
        out.value["sha256"].as_str().expect("sha256").to_owned(),
        std::fs::read(graph).expect("graph"),
    )
}

#[test]
fn snapshot_bytes_are_the_same_at_any_width() {
    let dir = Fixture::new();
    std::fs::create_dir_all(dir.0.join(".git")).expect("git marker");
    let files = (0..48)
        .map(|id| {
            let next = (id + 1) % 48;
            (
                format!("m{id:02}.ts"),
                format!(
                    "import {{ f{next} }} from './m{next:02}';\nexport function f{id}(n: number): number {{\n  const inner = (k: number) => k + {id};\n  return n > 0 ? f{next}(inner(n - 1)) : 0;\n}}\n"
                ),
            )
        })
        .collect::<Vec<_>>();
    let files = files
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    crate::tools::ast_graph::test_support::write_under(&dir, "app", &files);
    let root = dir.0.canonicalize().expect("root");
    let wide = build(&root);
    // Every core counted busy: each fan-out of this build takes one thread.
    let busy = (0..cpu::host_cores() * 2)
        .map(|_| cpu::host().enter())
        .collect::<Vec<_>>();
    assert_eq!(crate::tools::workers::workers(usize::MAX, None), 1);
    let narrow = build(&root);
    drop(busy);
    assert_eq!(wide.0, narrow.0, "the digest is the same at width 1");
    assert!(
        wide.1 == narrow.1,
        "the snapshot bytes are the same at width 1"
    );
}
