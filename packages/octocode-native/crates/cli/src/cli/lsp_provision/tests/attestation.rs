//! Release attestation: only a verified attestation that does not cover the
//! asset's digest refuses an install. Every other `gh` outcome (not logged
//! in, too old for the subcommand, offline, no attestation, no `gh`) checked
//! nothing and is reported as not checked. `gh` is a script in a temp dir,
//! handed to [`GitHub`] as its program.
use super::*;

const TAG: &str = "2026-06-22";
const REPO: &str = "rust-lang/rust-analyzer";

/// A `gh` that insists on the exact verify invocation, then runs `body`.
#[cfg(unix)]
fn fake_gh(body: &str) -> (tempfile::TempDir, GitHub) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let gh = dir.path().join("gh");
    std::fs::write(
        &gh,
        format!(
            "#!/bin/sh\n[ \"$*\" = \"release verify {TAG} -R {REPO} --format json\" ] || {{ echo \"unexpected arguments: $*\" >&2; exit 2; }}\n{body}\n"
        ),
    )
    .expect("script");
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let github = GitHub {
        gh: gh.into_os_string(),
        ..GitHub::new(None)
    };
    (dir, github)
}

/// The JSON `gh release verify --format json` prints for a verified
/// attestation whose asset subjects carry `digests` (the release subject has
/// only a sha1, as observed on an immutable cli/cli release).
fn verified_listing(digests: &[&str]) -> String {
    let mut subjects = vec![serde_json::json!({
        "uri": format!("pkg:github/{REPO}@{TAG}"),
        "digest": { "sha1": "0".repeat(40) },
    })];
    subjects.extend(digests.iter().enumerate().map(|(index, digest)| {
        serde_json::json!({ "name": format!("asset-{index}"), "digest": { "sha256": digest } })
    }));
    serde_json::json!({
        "attestation": {},
        "verificationResult": { "statement": { "subject": subjects } },
    })
    .to_string()
}

fn prints(json: &str) -> String {
    format!("cat <<'JSON'\n{json}\nJSON")
}

fn fails(stderr: &str) -> String {
    format!("echo '{stderr}' >&2\nexit 1")
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_verified_attestation_decides_by_the_assets_digest() {
    let ours = octocode_engine::digest::sha256(b"our asset");
    let theirs = octocode_engine::digest::sha256(b"another asset");

    let (_dir, covering) = fake_gh(&prints(&verified_listing(&[&theirs, &ours])));
    assert_eq!(
        covering.attest(REPO, TAG, &ours).await,
        Attestation::Verified
    );

    let (_dir, not_covering) = fake_gh(&prints(&verified_listing(&[&theirs])));
    let Attestation::Failed(reason) = not_covering.attest(REPO, TAG, &ours).await else {
        panic!("an attestation without our digest must refuse");
    };
    assert!(reason.contains(&ours), "{reason}");
}

/// Nothing was verified, so nothing contradicts the asset: not checked, with
/// gh's own words as the reason.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn when_gh_cannot_verify_the_attestation_is_not_checked_with_the_reason() {
    let sha = octocode_engine::digest::sha256(b"our asset");
    for (body, reason) in [
        (fails("HTTP 401: Bad credentials"), "HTTP 401"),
        (
            fails("unknown command \"verify\" for \"gh release\""),
            "unknown command",
        ),
        (fails("no attestations found"), "no attestations found"),
        (
            fails("dial tcp: lookup api.github.com: no such host"),
            "no such host",
        ),
        (prints("this is not JSON"), "shape this build does not read"),
        (
            prints("{\"verificationResult\":{}}"),
            "shape this build does not read",
        ),
    ] {
        let (_dir, github) = fake_gh(&body);
        let Attestation::NotChecked(said) = github.attest(REPO, TAG, &sha).await else {
            panic!("{body}: a gh that verified nothing must not refuse");
        };
        assert!(said.contains(reason), "{reason}: {said}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_missing_gh_is_not_checked() {
    let github = GitHub {
        gh: "/nonexistent/octocode-test-gh".into(),
        ..GitHub::new(None)
    };
    let outcome = github.attest(REPO, TAG, &"0".repeat(64)).await;
    assert!(
        matches!(&outcome, Attestation::NotChecked(reason) if reason.contains("not on PATH")),
        "{outcome:?}"
    );
}
