use super::*;
use octocode_runtime_base::execution::cancel::NeverCancel;

struct CancelAfterFirst(std::sync::atomic::AtomicUsize);
impl CancellationCheck for CancelAfterFirst {
    fn check(&self) -> Result<(), String> {
        if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            Ok(())
        } else {
            Err("cancelled".into())
        }
    }
}

#[test]
fn utf16_capture_preserves_decoded_text_and_raw_snapshot() {
    let root = tempfile::tempdir().expect("root");
    let file = root.path().join("source.txt");
    let paths = super::super::tests::Paths(root.path().into());
    let query = super::super::tests::q(&file);
    let source = "α😀\r\nsecond\n";
    for little in [true, false] {
        let mut bytes = if little {
            vec![0xff, 0xfe]
        } else {
            vec![0xfe, 0xff]
        };
        for unit in source.encode_utf16() {
            bytes.extend(if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
        std::fs::write(&file, &bytes).expect("write");
        let captured = capture_text(&query, &paths, &NeverCancel).expect("capture");
        assert_eq!(captured.text, source);
        assert_eq!(captured.snapshot, octocode_engine::digest::sha256(&bytes));
        bytes.push(1);
        std::fs::write(&file, &bytes).expect("malformed");
        assert_eq!(
            capture_text(&query, &paths, &NeverCancel)
                .unwrap_err()
                .error_code,
            Some(ErrorCode::BinaryFileUnsupported)
        );
    }
}

#[test]
fn capture_preserves_complete_source_bytes_and_rejects_stale_snapshots() {
    let root = tempfile::tempdir().expect("root");
    let file = root.path().join("source.txt");
    let source = "α\r\nSECRET\n🙂\n";
    std::fs::write(&file, source).expect("write");
    let paths = super::super::tests::Paths(root.path().into());
    let mut query = super::super::tests::q(&file);
    local_fetch_query::set_line_span(&mut query, 2, 2).expect("range");
    let captured = capture_text(&query, &paths, &NeverCancel).expect("capture");
    assert_eq!(captured.text, source);
    assert_eq!(captured.canonical, file.canonicalize().expect("canonical"));
    assert_eq!(
        captured.snapshot,
        octocode_engine::digest::sha256(source.as_bytes())
    );
    local_fetch_query::set_snapshot(&mut query, Some(&captured.snapshot)).expect("snapshot");
    assert!(capture_text(&query, &paths, &NeverCancel).is_ok());
    std::fs::write(&file, "changed").expect("change");
    assert_eq!(
        capture_text(&query, &paths, &NeverCancel)
            .unwrap_err()
            .error_code,
        Some(ErrorCode::StaleSnapshot)
    );
}

#[test]
fn capture_refuses_binary_invalid_utf8_oversize_and_cancelled_sources() {
    let root = tempfile::tempdir().expect("root");
    let file = root.path().join("source.txt");
    let paths = super::super::tests::Paths(root.path().into());
    let query = super::super::tests::q(&file);
    let mut binary = vec![b'a'; 9000];
    binary.push(0);
    for (bytes, code) in [
        (binary, ErrorCode::BinaryFileUnsupported),
        (
            vec![b'a', 0xff],
            ErrorCode::ClassificationContextUnavailable,
        ),
    ] {
        std::fs::write(&file, bytes).expect("write");
        assert_eq!(
            capture_text(&query, &paths, &NeverCancel)
                .unwrap_err()
                .error_code,
            Some(code)
        );
    }
    std::fs::File::create(&file)
        .expect("file")
        .set_len(MAX_SOURCE_BYTES + 1)
        .expect("size");
    assert_eq!(
        capture_text(&query, &paths, &NeverCancel)
            .unwrap_err()
            .error_code,
        Some(ErrorCode::ClassificationContextTooLarge)
    );
    std::fs::write(&file, "text").expect("write");
    let cancel = CancelAfterFirst(std::sync::atomic::AtomicUsize::new(0));
    assert_eq!(
        capture_text(&query, &paths, &cancel)
            .unwrap_err()
            .error_code,
        Some(ErrorCode::Cancelled)
    );
}

#[test]
fn capture_enforces_the_workspace_path_policy_and_regular_files() {
    let root = tempfile::tempdir().expect("root");
    let outside = tempfile::NamedTempFile::new().expect("outside");
    let paths = octocode_runtime_base::policy::path::PathPolicy::new(
        octocode_runtime_base::policy::path::PathPolicyConfig {
            workspace_root: Some(root.path().into()),
            ..Default::default()
        },
    )
    .expect("policy");
    assert!(
        capture_text(
            &super::super::tests::q(outside.path()),
            &paths,
            &NeverCancel
        )
        .is_err()
    );
    assert!(capture_text(&super::super::tests::q(root.path()), &paths, &NeverCancel).is_err());
}
