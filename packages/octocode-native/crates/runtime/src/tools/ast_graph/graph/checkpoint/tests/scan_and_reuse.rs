//! What a checkpoint writes: artifacts past one secret scan, and reused
//! artifacts kept rather than written again.
use super::*;

/// `count` exported functions, each calling the next: dense facts, so its
/// extraction artifact outgrows what one secret scan admits.
fn dense_source(count: usize, tail: &str) -> String {
    let mut text = (0..count)
        .map(|id| {
            format!(
                "export function f{id}(a, b) {{ return f{}(b, a); }}\n",
                id + 1
            )
        })
        .collect::<String>();
    text.push_str(tail);
    text
}

/// An artifact larger than one secret scan admits is scanned in windows:
/// without a secret it is checkpointed on disk like any other, and with one
/// anywhere in it, it never is.
#[test]
fn an_artifact_past_one_scan_is_scanned_in_windows() {
    let (dir, scan) = fixture();
    dir.write("big.ts", dense_source(7_000, ""));
    let clean = build(&scan, &NeverCancel).unwrap();
    assert_eq!(clean.construction.memory_only_files, 0);
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    let artifact = job_dir.join(format!(
        "{}.fact",
        octocode_engine::digest::sha256(b"big.ts")
    ));
    let size = std::fs::metadata(&artifact)
        .expect("big.ts checkpointed")
        .len();
    assert!(
        size > 10_000_000,
        "the artifact must outgrow one scan: {size}"
    );

    let token = format!("ghp_{}", &"a1B2c3D4e5F6g7H8i9J0".repeat(2)[..36]);
    dir.write(
        "big.ts",
        // An import specifier is kept in the facts, so the token reaches the
        // artifact.
        dense_source(7_000, &format!("import key from '{token}';\n")),
    );
    let secret = build(&scan, &NeverCancel).unwrap();
    assert_eq!(secret.construction.memory_only_files, 1);
    assert!(!artifact.exists());
}

/// Windows overlap, so a secret placed across a window boundary is still
/// read whole; a text with none is clean however many windows it spans.
#[test]
fn a_secret_across_a_window_boundary_is_found() {
    let token = format!("ghp_{}", &"a1B2c3D4e5F6g7H8i9J0".repeat(2)[..36]);
    let filler = "x ".repeat(SECRET_SCAN_WINDOW / 2);
    let at = SECRET_SCAN_WINDOW - token.len() / 2;
    let text = format!("{}{token} tail", &filler[..at]);
    let security = ContentSecurity::new();
    assert!(holds_secret(&security, &text));
    assert!(!holds_secret(
        &security,
        &"word ".repeat(SECRET_SCAN_WINDOW / 2)
    ));
}

/// A file reused from its own verified artifact keeps that artifact: after
/// one edit, only the edited file's artifact is written again, and the build
/// still equals a clean one.
#[test]
fn reused_artifacts_are_kept_not_rewritten() {
    let (dir, scan) = fixture();
    build(&scan, &NeverCancel).unwrap();
    let job_dir = job_path(scan.checkpoint.as_ref().unwrap(), &scan.root);
    let artifact = |name: &str| {
        job_dir.join(format!(
            "{}.fact",
            octocode_engine::digest::sha256(name.as_bytes())
        ))
    };
    let written = |name: &str| {
        std::fs::metadata(artifact(name))
            .unwrap()
            .modified()
            .unwrap()
    };
    let before = written("f01.ts");
    std::thread::sleep(std::time::Duration::from_millis(20));
    dir.write("f00.ts", "export function f0() { return 100; }\n");
    let edited = build(&scan, &NeverCancel).unwrap();
    assert_eq!(
        written("f01.ts"),
        before,
        "an unchanged file's artifact was rewritten"
    );
    assert_eq!(edited.construction.checkpointed_files, 12);
    let clean = build(
        &GraphScan {
            checkpoint: None,
            ..scan.clone()
        },
        &NeverCancel,
    )
    .unwrap();
    equivalent(edited, clean);
}
