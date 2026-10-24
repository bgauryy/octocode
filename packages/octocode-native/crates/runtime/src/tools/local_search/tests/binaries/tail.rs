//! A deferred hit before a large binary tail: the page reconstructs the
//! hit from the stored scan, the drain stays cancellable, and the tail's
//! bytes stay bound by the source digest.
use super::*;
use crate::execution::cancel::CancellationCheck;
use crate::tools::local_search::{executor, verify};
use octocode_engine::types::TextSearchFile;
use std::io::{Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Reconstruct `file`'s first row from its source under `cancel`, with no
/// stored digest to check against.
fn materialize_first_row<C: CancellationCheck>(
    request: &LocalSearchQuery,
    policy: &PathPolicy,
    root: &std::path::Path,
    security: &ContentSecurity,
    cancel: &C,
    file: &mut TextSearchFile,
) -> Result<(), crate::response::result::ToolError> {
    let expected = |_: &std::path::Path| crate::tools::local_search::sources::Expected::Fresh;
    let scope = verify::PageScope {
        query: request,
        paths: policy,
        output_root: root,
        expected_digest: &expected,
        security,
        cancel,
        walk_threads: Some(1),
    };
    verify::materialize(&scope, file, 0..1)
}

#[test]
fn deferred_hit_before_a_large_binary_tail_remains_readable_and_binds_the_tail() {
    let root = tempfile::tempdir().expect("fixture");
    let path = root.path().join("binary-tail.txt");
    let mut source = fs::File::create(&path).expect("create");
    source
        .write_all(b"needle before binary\n\0")
        .expect("prefix");
    let chunk = vec![b'x'; 64 * 1024];
    for _ in 0..(65 * 16) {
        source.write_all(&chunk).expect("newline-free tail");
    }
    drop(source);
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path":path,"matchString":["needle"]}),
        None,
    );
    let scan = executor::scan_query(&request, &policy, &NeverCancel, Some(1), None)
        .expect("engine accepts the binary prefix");
    assert_eq!(scan.parsed.files.len(), 1);
    assert_eq!(scan.parsed.files[0].matches.len(), 1);
    assert!(scan.parsed.files[0].matches[0].deferred.is_some());
    assert!(
        scan.parsed
            .stats
            .cap_reasons
            .contains(&octocode_engine::types::CapReason::BinaryQuit)
    );
    let original = scan.parsed.files[0].clone();
    let mut reconstructed = original.clone();
    materialize_first_row(
        &request,
        &policy,
        root.path(),
        &security,
        &NeverCancel,
        &mut reconstructed,
    )
    .expect("unchanged binary tail does not prevent reconstructing the earlier hit");
    assert_eq!(reconstructed.matches[0].value, "needle before binary");

    let result = executor::page_of(
        &request,
        &policy,
        &security,
        &NeverCancel,
        Some(1),
        scan,
        None,
    )
    .expect("unchanged binary tail is hashed without buffering it as a line");
    let mut owned_scan = executor::scan_query(&request, &policy, &NeverCancel, Some(1), None)
        .expect("baseline scan plan");
    let mut options = owned_scan.search_options.clone();
    options.values = octocode_engine::types::SourceValues::Hashed;
    owned_scan.parsed =
        executor::scan(&request, &policy, options, &policy.identity(), &NeverCancel)
            .expect("owned engine baseline")
            .parsed;
    assert_eq!(owned_scan.parsed.files[0].matches[0].deferred, None);
    assert_eq!(
        owned_scan.parsed.files[0].matches[0].value,
        "needle before binary"
    );
    owned_scan.reusable = None;
    let owned = executor::page_of(
        &request,
        &policy,
        &security,
        &NeverCancel,
        Some(1),
        owned_scan,
        None,
    )
    .expect("owned page preserves the existing conservative secret guard");
    assert_eq!(
        result, owned,
        "deferred reconstruction preserves the complete owned page, including redaction"
    );

    struct CancelDuringDrain(AtomicUsize);
    impl CancellationCheck for CancelDuringDrain {
        fn check(&self) -> Result<(), String> {
            if self.0.fetch_add(1, Ordering::Relaxed) >= 32 {
                Err("tail-drain-cancel".into())
            } else {
                Ok(())
            }
        }
    }
    let cancel = CancelDuringDrain(AtomicUsize::new(0));
    let error = materialize_first_row(
        &request,
        &policy,
        root.path(),
        &security,
        &cancel,
        &mut original.clone(),
    )
    .expect_err("digest-only drain remains cancellable");
    assert!(format!("{error:?}").contains("tail-drain-cancel"));

    let mut source = fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open tail");
    source.seek(SeekFrom::End(-1)).expect("last byte");
    source.write_all(b"z").expect("same-size tail edit");
    drop(source);
    assert!(
        materialize_first_row(
            &request,
            &policy,
            root.path(),
            &security,
            &NeverCancel,
            &mut original.clone()
        )
        .is_err(),
        "even unshown tail bytes remain bound by the source digest"
    );
}
