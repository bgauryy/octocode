//! The install-time three-way check: the lock, GitHub's live digest (served
//! here by a local HTTP fixture through the real API client), and the hash of
//! the downloaded bytes.
use super::*;
use octocode_engine::lsp::managed::ManifestServer;

const PLATFORM: &str = "linux-x64";

/// The real rust-analyzer entry, locked to `download`'s own digest, so a
/// test can install it end to end.
fn locked_to(download: &[u8]) -> (ManifestServer, ManifestAsset) {
    let server = manifest_server("rust-analyzer").expect("server").clone();
    let mut asset = server.platforms[PLATFORM].clone();
    asset.lock = Some(AssetLock {
        url: "https://github.com/rust-lang/rust-analyzer/releases/download/x/asset.gz".into(),
        sha256: octocode_engine::digest::sha256(download),
        size: download.len() as u64,
    });
    (server, asset)
}

/// The release JSON GitHub's API returns, reporting `digest` for `asset`.
fn release_json(asset: &str, digest: &str) -> String {
    serde_json::json!({
        "tag_name": "x",
        "immutable": false,
        "assets": [
            { "name": "unrelated.zip", "digest": format!("sha256:{}", "1".repeat(64)) },
            { "name": asset, "digest": format!("sha256:{digest}") },
        ],
    })
    .to_string()
}

fn api(base: String) -> Option<GitHub> {
    Some(GitHub {
        api_base: base,
        token: None,
    })
}

async fn provision(root: &std::path::Path, download: &[u8], upstream: Fake) -> ProvisionOutcome {
    let (server, asset) = locked_to(download);
    provision_entry(
        root,
        "rust-analyzer",
        PLATFORM,
        &server,
        &asset,
        LspAutoInstall::Auto,
        &upstream,
    )
    .await
}

#[tokio::test(flavor = "current_thread")]
async fn an_install_where_lock_live_digest_and_bytes_agree_succeeds() {
    let payload = b"rust-analyzer-binary";
    let download = gz(payload);
    let (_, asset) = locked_to(&download);
    let live = serve(
        200,
        release_json(&asset.asset, &octocode_engine::digest::sha256(&download)),
    );
    let dir = tempfile::tempdir().expect("tempdir");
    let outcome = provision(
        dir.path(),
        &download,
        Fake {
            live: api(live),
            ..Fake::serving(download.clone())
        },
    )
    .await;
    assert!(outcome.ok, "{outcome:?}");
    let bin = std::path::PathBuf::from(outcome.path.expect("path"));
    assert_eq!(std::fs::read(&bin).expect("binary"), payload);
    let verification = outcome.verification.expect("verification");
    assert!(
        verification.contains("live digest agrees"),
        "{verification}"
    );
    // The marker records the asset identity the install verified.
    let marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(marker_path(&bin)).expect("marker")).expect("json");
    assert_eq!(marker["asset"], asset.asset);
    assert_eq!(
        marker["assetSha256"],
        octocode_engine::digest::sha256(&download)
    );
    assert_eq!(marker["repo"], "rust-lang/rust-analyzer");
}

#[tokio::test(flavor = "current_thread")]
async fn a_live_digest_that_differs_from_the_lock_is_refused_as_changed_upstream() {
    let download = gz(b"locked bytes");
    let (_, asset) = locked_to(&download);
    let replaced = octocode_engine::digest::sha256(b"replaced upstream");
    let dir = tempfile::tempdir().expect("tempdir");
    let outcome = provision(
        dir.path(),
        &download,
        Fake {
            live: api(serve(200, release_json(&asset.asset, &replaced))),
            ..Fake::never()
        },
    )
    .await;
    let error = outcome.error.expect("refused");
    assert!(
        error.contains("changed upstream since this Octocode build"),
        "{error}"
    );
    assert!(error.contains(&replaced), "names the live digest: {error}");
    assert!(
        error.contains(&octocode_engine::digest::sha256(&download)),
        "names the locked digest: {error}"
    );
    // A release that lost the asset is a change upstream too.
    let outcome = provision(
        dir.path(),
        &download,
        Fake {
            live: api(serve(404, "{}".into())),
            ..Fake::never()
        },
    )
    .await;
    assert!(outcome.error.expect("refused").contains("changed upstream"));
}

#[tokio::test(flavor = "current_thread")]
async fn bytes_that_differ_from_the_lock_are_refused_as_corrupted() {
    let download = gz(b"locked bytes");
    let (_, asset) = locked_to(&download);
    let live = serve(
        200,
        release_json(&asset.asset, &octocode_engine::digest::sha256(&download)),
    );
    let dir = tempfile::tempdir().expect("tempdir");
    let outcome = provision(
        dir.path(),
        &download,
        Fake {
            live: api(live),
            ..Fake::serving(gz(b"substituted bytes"))
        },
    )
    .await;
    let error = outcome.error.expect("refused");
    assert!(error.contains("corrupted or substituted"), "{error}");
    assert_eq!(install_dir_leftovers(dir.path()), Vec::<String>::new());
}

/// GitHub unreachable (connection refused, or a rate-limit answer): the bytes
/// are still checked against the lock, the install succeeds, and the outcome
/// says the live cross-check was skipped.
#[tokio::test(flavor = "current_thread")]
async fn an_unreachable_api_installs_from_the_lock_and_says_so() {
    let download = gz(b"rust-analyzer-binary");
    for base in [
        "http://127.0.0.1:1".to_owned(),
        serve(403, "{\"message\":\"rate limit\"}".into()),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = provision(
            dir.path(),
            &download,
            Fake {
                live: api(base.clone()),
                ..Fake::serving(download.clone())
            },
        )
        .await;
        assert!(outcome.ok, "{base}: {outcome:?}");
        let verification = outcome.verification.expect("verification");
        assert!(verification.contains("was not checked"), "{verification}");
        assert!(
            verification.contains("managed.lock.json only"),
            "{verification}"
        );
    }
    // The same download with corrupted bytes is still refused.
    let dir = tempfile::tempdir().expect("tempdir");
    let outcome = provision(
        dir.path(),
        &download,
        Fake {
            live: api("http://127.0.0.1:1".into()),
            ..Fake::serving(gz(b"corrupted"))
        },
    )
    .await;
    assert!(outcome.error.expect("refused").contains("corrupted"));
}

/// An immutable release's attestation is checked; a failing one refuses.
#[tokio::test(flavor = "current_thread")]
async fn an_immutable_release_verifies_its_attestation() {
    let download = gz(b"rust-analyzer-binary");
    let (mut server, asset) = locked_to(&download);
    server.release_lock = Some(octocode_engine::lsp::managed::ReleaseLock {
        immutable: true,
        published_at: String::new(),
    });
    let cases = [
        (Attestation::Verified, Ok("release attestation verified")),
        (
            Attestation::NotChecked("gh is not on PATH".into()),
            Ok("was not checked (gh is not on PATH)"),
        ),
        (
            Attestation::Failed("bad signer".into()),
            Err("does not verify: bad signer"),
        ),
    ];
    for (attestation, expected) in cases {
        let dir = tempfile::tempdir().expect("tempdir");
        let live = serve(
            200,
            release_json(&asset.asset, &octocode_engine::digest::sha256(&download)),
        );
        let upstream = Fake {
            live: api(live),
            attestation,
            ..Fake::serving(download.clone())
        };
        let outcome = provision_entry(
            dir.path(),
            "rust-analyzer",
            PLATFORM,
            &server,
            &asset,
            LspAutoInstall::Auto,
            &upstream,
        )
        .await;
        match expected {
            Ok(text) => assert!(outcome.verification.expect("ok").contains(text)),
            Err(text) => assert!(outcome.error.expect("refused").contains(text)),
        }
    }
}

#[test]
fn the_api_host_is_not_a_download_host() {
    assert!(!host_allowed(
        "https://api.github.com/repos/a/b/releases/tags/c"
    ));
    assert!(!host_allowed("https://releases.hashicorp.com/x.zip"));
    assert!(super::super::upstream::GITHUB_API.starts_with("https://api.github.com"));
}
