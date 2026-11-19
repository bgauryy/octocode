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
        ..GitHub::new(None)
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
    let failure = outcome.failure.expect("refused");
    assert_eq!(failure.code, ErrorCode::SecurityValidationFailed);
    let error = failure.message;
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
    let failure = outcome.failure.expect("refused");
    assert!(failure.message.contains("changed upstream"), "{failure:?}");
}

/// Bytes of the locked size whose hash is not the locked one are refused by
/// the hash check; bytes of another size are refused before any hashing.
#[tokio::test(flavor = "current_thread")]
async fn bytes_that_differ_from_the_lock_are_refused_as_corrupted() {
    let download = gz(b"locked bytes");
    let (_, asset) = locked_to(&download);
    let mut same_size = download.clone();
    *same_size.last_mut().expect("bytes") ^= 0xff;
    let mut longer = download.clone();
    longer.push(0);
    for (served, refusal) in [
        (same_size, "sha256"),
        (longer, "bytes but managed.lock.json records"),
    ] {
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
                ..Fake::serving(served)
            },
        )
        .await;
        let failure = outcome.failure.expect("refused");
        assert_eq!(failure.code, ErrorCode::SecurityValidationFailed);
        let error = failure.message;
        assert!(error.contains("corrupted or substituted"), "{error}");
        assert!(error.contains(refusal), "{refusal}: {error}");
        assert_eq!(install_dir_leftovers(dir.path()), Vec::<String>::new());
    }
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
    let failure = outcome.failure.expect("refused");
    assert!(failure.message.contains("corrupted"), "{failure:?}");
}

/// An immutable release's attestation is checked; a failing one refuses.
#[tokio::test(flavor = "current_thread")]
async fn an_immutable_release_verifies_its_attestation() {
    let download = gz(b"rust-analyzer-binary");
    let (mut server, asset) = locked_to(&download);
    server.release_lock = Some(octocode_engine::lsp::managed::ReleaseLock { immutable: true });
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
            Err(text) => {
                let failure = outcome.failure.expect("refused");
                assert!(failure.message.contains(text), "{failure:?}");
            }
        }
    }
}

/// The GitHub token authenticates the metadata request to the API host and
/// goes nowhere else. The download path takes no token at all
/// (`download::fetch_allowlisted(url, file, limit)`), and a host outside the release
/// allowlist, like this local fixture, is refused before any request is sent.
#[tokio::test(flavor = "current_thread")]
async fn the_token_goes_to_the_api_host_and_never_to_a_download_host() {
    let download = gz(b"rust-analyzer-binary");
    let (_, asset) = locked_to(&download);
    let sha = octocode_engine::digest::sha256(&download);
    let (base, api_requests) = serve_recording(200, release_json(&asset.asset, &sha));
    let github = GitHub {
        api_base: base,
        token: Some("ghp_secret_token".to_owned()),
        ..GitHub::new(None)
    };
    let live = github
        .live_digest("rust-lang/rust-analyzer", "x", &asset.asset)
        .await;
    assert_eq!(live, Ok(Some(sha)));
    {
        let requests = api_requests.lock().expect("requests");
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0]
                .to_ascii_lowercase()
                .contains("authorization: bearer ghp_secret_token"),
            "{}",
            requests[0]
        );
    }

    let (download_base, download_requests) = serve_recording(200, "bytes".into());
    let dir = tempfile::tempdir().expect("tempdir");
    let spool = tokio::fs::File::from_std(
        octocode_io::atomic::create_new(&dir.path().join("spool"), 0o600).expect("spool"),
    );
    let refused = github
        .download(&format!("{download_base}/asset.gz"), spool, u64::MAX)
        .await
        .expect_err("a host outside the allowlist is refused");
    assert!(
        refused.message.contains("Blocked non-allowlisted"),
        "{refused:?}"
    );
    assert!(download_requests.lock().expect("requests").is_empty());
}

/// The release JSON is read under the provider body bound: an answer past it
/// is refused as unchecked, even one that names the asset's digest, and the
/// read stops there instead of buffering whatever the server sends.
#[tokio::test(flavor = "current_thread")]
async fn a_release_json_past_the_provider_body_bound_is_not_read() {
    let limit = octocode_io::budget::PROVIDER_BODY_BYTES;
    let sha = "a".repeat(64);
    let oversized = serde_json::json!({
        "tag_name": "x",
        "body": "x".repeat(limit),
        "assets": [{ "name": "asset.gz", "digest": format!("sha256:{sha}") }],
    })
    .to_string();
    assert!(oversized.len() > limit);
    let github = GitHub {
        api_base: serve(200, oversized),
        ..GitHub::new(None)
    };
    let live = github
        .live_digest("rust-lang/rust-analyzer", "x", "asset.gz")
        .await;
    let reason = live.expect_err("an oversized release JSON is not read");
    assert!(
        reason.contains(&format!("exceeds the {limit}-byte limit")),
        "{reason}"
    );

    let within = GitHub {
        api_base: serve(200, release_json("asset.gz", &sha)),
        ..GitHub::new(None)
    };
    assert_eq!(
        within
            .live_digest("rust-lang/rust-analyzer", "x", "asset.gz")
            .await,
        Ok(Some(sha))
    );
}

#[test]
fn the_api_host_is_not_a_download_host() {
    assert!(!host_allowed(
        "https://api.github.com/repos/a/b/releases/tags/c"
    ));
    assert!(!host_allowed("https://releases.hashicorp.com/x.zip"));
    assert!(super::super::upstream::GITHUB_API.starts_with("https://api.github.com"));
}
