use super::*;

fn field(number: u64, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    put_varint(&mut out, number << 3 | 2);
    put_varint(&mut out, payload.len() as u64);
    out.extend_from_slice(payload);
    out
}

pub(crate) fn index_of(paths: &[&str]) -> Vec<u8> {
    let mut out = field(1, &[]);
    for path in paths {
        out.extend(field(2, &field(1, path.as_bytes())));
    }
    out
}

#[test]
fn a_member_crate_scope_reads_the_workspace_index_by_scope_relative_paths() {
    let root = std::path::Path::new("/ws");
    let scope = std::path::Path::new("/ws/crates/a");
    assert_eq!(scope_prefix(root, scope).as_deref(), Ok("crates/a"));
    assert_eq!(scope_prefix(root, root).as_deref(), Ok(""));

    let bytes = index_of(&["crates/a/src/lib.rs", "crates/b/src/lib.rs", "Cargo.toml"]);
    let rebased = rebase(&bytes, "crates/a").expect("rebase");
    let index = ScipIndex::parse(&rebased).expect("parse");
    assert!(
        index.has_document("src/lib.rs"),
        "the scope's file keeps its graph key"
    );
    assert!(!index.has_document("crates/a/src/lib.rs"));
    assert!(
        !index.has_document("crates/b/src/lib.rs"),
        "outside the scope: skipped"
    );
    // A scope that is the SCIP root changes nothing.
    assert_eq!(rebase(&bytes, "").expect("rebase"), bytes);
    assert!(
        rebase(&[0x12, 0x7f], "crates/a").is_err(),
        "a truncated index is an error"
    );
}
