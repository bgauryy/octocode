//! A materialize that reads blobs through git ends where the REST path ends:
//! the same files, bytes, skips and manifest, in a request or two instead of
//! one read per file. The remote is a local bare repository; the REST path
//! is the mock server.
use crate::gh_structure::materialize::{MaterializeOutcome, Snapshot, materialize_tree};
use crate::gh_structure::{EntryKind, TreeEntry};
use octocode_github::{CredentialSource, RequestContext, ResolvedCredential};
use octocode_io::budget::RequestBudget;
use octocode_runtime_base::execution::cancel::{CancellationCheck, NeverCancel};
use octocode_runtime_base::policy::path::{PathPolicy, PathPolicyConfig};
use octocode_runtime_base::test_support::Fixture;
use octocode_test_support::json_fixture;
use octocode_tools_shared::gh_git::blobs::BlobSource;
use octocode_tools_shared::gh_git::{
    GitConfig, GitError, GitOutput, GitRunControl, GitRunRequest, GitRunner, GitSetup, SystemGit,
};
use octocode_tools_shared::gh_shared::test_support::{mock_endpoint, one_shot_provider};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// What this call may read: the REST path's body limit, and the size a wave
/// adds up to.
const READ_LIMIT: usize = 1024;
const TOKEN: &str = "ghs_fixturesecrettoken";

/// A git remote on disk and the listing GitHub's tree API would give for it.
struct Remote {
    _directory: Fixture,
    bare: PathBuf,
    commit: String,
    /// Files in listing order, with their bodies.
    files: Vec<(String, Vec<u8>)>,
    entries: Vec<TreeEntry>,
}

fn git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(directory)
        .env_clear()
        .env("HOME", directory)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
        .args(args)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

impl Remote {
    /// `files` committed, plus a symlink `link` and a submodule `sub` that a
    /// listing leaves out, `tool.sh` executable.
    fn new(files: &[(&str, Vec<u8>)]) -> Self {
        let directory = Fixture::new();
        let work = directory.0.join("work");
        let bare = directory.0.join("remote.git");
        std::fs::create_dir_all(&work).unwrap();
        git(&work, &["init", "--quiet", "--initial-branch=main"]);
        for (name, body) in files {
            directory.write(&format!("work/{name}"), body);
        }
        git(&work, &["add", "."]);
        let target = work.join("link-target");
        std::fs::write(&target, b"src/a.rs").unwrap();
        let link = git(&work, &["hash-object", "-w", "link-target"]);
        std::fs::remove_file(&target).unwrap();
        git(
            &work,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("120000,{},link", link.trim()),
            ],
        );
        git(
            &work,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                "160000,1111111111111111111111111111111111111111,sub",
            ],
        );
        git(&work, &["update-index", "--chmod=+x", "tool.sh"]);
        git(&work, &["commit", "--quiet", "-m", "fixture"]);
        let commit = git(&work, &["rev-parse", "HEAD"]).trim().to_owned();
        git(
            &directory.0,
            &[
                "clone",
                "--quiet",
                "--bare",
                work.to_str().unwrap(),
                bare.to_str().unwrap(),
            ],
        );
        // What a remote that serves a blob by id does (GitHub does).
        git(&bare, &["config", "uploadpack.allowAnySHA1InWant", "true"]);
        let listing = git(&work, &["ls-tree", "-r", "HEAD"]);
        let mut entries = Vec::new();
        for line in listing.lines() {
            let (meta, name) = line.split_once('\t').unwrap();
            let mut meta = meta.split(' ');
            let (mode, kind, oid) = (
                meta.next().unwrap(),
                meta.next().unwrap(),
                meta.next().unwrap(),
            );
            // As the tree listing: a symlink and a submodule are not files.
            if kind != "blob" || mode == "120000" {
                continue;
            }
            let body = &files.iter().find(|(file, _)| *file == name).unwrap().1;
            entries.push(TreeEntry {
                path: name.to_owned(),
                kind: EntryKind::File,
                size: Some(body.len() as u64),
                oid: Some(oid.to_owned()),
            });
        }
        let files = entries
            .iter()
            .map(|entry| {
                let body = &files
                    .iter()
                    .find(|(file, _)| *file == entry.path)
                    .unwrap()
                    .1;
                (entry.path.clone(), body.clone())
            })
            .collect();
        Self {
            _directory: directory,
            bare,
            commit,
            files,
            entries,
        }
    }

    fn url(&self) -> String {
        url::Url::from_file_path(&self.bare).unwrap().to_string()
    }

    /// The REST path's answer for every file: its bytes, the blob id as ETag.
    async fn serve(&self, server: &MockServer) {
        for entry in &self.entries {
            let body = &self
                .files
                .iter()
                .find(|(file, _)| *file == entry.path)
                .unwrap()
                .1;
            Mock::given(method("GET"))
                .and(path(format!(
                    "/api/v3/repos/octo/demo/contents/{}",
                    entry.path.replace('/', "%2F")
                )))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "application/vnd.github.raw")
                        .insert_header("etag", format!("\"{}\"", entry.oid.as_ref().unwrap()))
                        .set_body_bytes(body.clone()),
                )
                .mount(server)
                .await;
        }
    }
}

/// The files of the mixed fixture: text in three encodings, a binary, an LFS
/// pointer, files the credential rule withholds, one past the read limit,
/// and an executable.
fn mixed() -> Vec<(&'static str, Vec<u8>)> {
    let utf16: Vec<u8> = [0xff, 0xfe]
        .into_iter()
        .chain("hi\n".encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    vec![
        (".env", b"TOKEN=abc\n".to_vec()),
        ("big.txt", vec![b'x'; READ_LIMIT * 2]),
        ("deploy/id_rsa", b"key\n".to_vec()),
        (
            "lfs.bin",
            b"version https://git-lfs.github.com/spec/v1\noid sha256:aa\nsize 9\n".to_vec(),
        ),
        ("pkg/.npmrc", b"//registry:_authToken=x\n".to_vec()),
        ("src/a.rs", b"fn a() {}\n".to_vec()),
        ("src/data.bin", b"\0\x01\x02binary".to_vec()),
        ("src/latin1.txt", b"caf\xe9\n".to_vec()),
        ("src/utf16.txt", utf16),
        ("tool.sh", b"#!/bin/sh\n".to_vec()),
    ]
}

/// What the fixture's git saw.
#[derive(Clone, Debug)]
struct Call {
    label: String,
    args: Vec<String>,
    authorization: Option<String>,
    authorization_url: Option<String>,
    stdin: Option<Vec<u8>>,
}

/// The system git, its GitHub URL answered by the local remote, recording
/// each call.
struct FixtureGit {
    system: SystemGit,
    remote: String,
    calls: Mutex<Vec<Call>>,
    /// A fetch asks for an id the remote does not have: a server that refuses.
    refuse_fetch: bool,
    /// Runs before each blob fetch, numbered from 1.
    before_fetch: Option<Box<dyn Fn(usize) + Send + Sync>>,
}

impl FixtureGit {
    fn new(remote: &Remote) -> Self {
        Self {
            system: SystemGit::default().with_file_transport(),
            remote: remote.url(),
            calls: Mutex::new(Vec::new()),
            refuse_fetch: false,
            before_fetch: None,
        }
    }

    fn calls(&self) -> Vec<Call> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn fetches(&self) -> Vec<Call> {
        self.calls()
            .into_iter()
            .filter(|call| call.label == "fetch blobs")
            .collect()
    }

    fn prepared<'a>(&self, request: &GitRunRequest<'a>) -> GitRunRequest<'a> {
        let call = Call {
            label: request.label.clone(),
            args: request
                .args
                .iter()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect(),
            authorization: request.authorization.map(str::to_owned),
            authorization_url: request.authorization_url.map(str::to_owned),
            stdin: request.stdin.map(<[u8]>::to_vec),
        };
        let number = {
            let mut calls = self.calls.lock().unwrap_or_else(PoisonError::into_inner);
            calls.push(call);
            calls
                .iter()
                .filter(|call| call.label == "fetch blobs")
                .count()
        };
        let fetching = request.label == "fetch blobs";
        if fetching && let Some(before) = &self.before_fetch {
            before(number);
        }
        let args: Vec<OsString> = request
            .args
            .iter()
            .map(|argument| {
                let text = argument.to_string_lossy();
                if text.starts_with("http://") || text.starts_with("https://") {
                    OsString::from(&self.remote)
                } else {
                    argument.clone()
                }
            })
            .collect();
        GitRunRequest {
            args,
            timeout: request.timeout,
            label: request.label.clone(),
            authorization: request.authorization,
            authorization_url: request.authorization_url,
            stdin: if fetching && self.refuse_fetch {
                Some(b"0000000000000000000000000000000000000001\n")
            } else {
                request.stdin
            },
        }
    }
}

impl GitRunner for FixtureGit {
    fn run(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
    ) -> Result<GitOutput, GitError> {
        self.system.run(&self.prepared(request), control)
    }

    fn run_nul_records(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
        record: &mut (dyn FnMut(&[u8]) -> Result<(), GitError> + Send),
    ) -> Result<GitOutput, GitError> {
        self.system
            .run_nul_records(&self.prepared(request), control, record)
    }

    fn run_stream(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
        sink: &mut (dyn FnMut(&[u8]) -> Result<(), GitError> + Send),
    ) -> Result<GitOutput, GitError> {
        self.system
            .run_stream(&self.prepared(request), control, sink)
    }
}

struct Cancel(Arc<AtomicBool>);
impl CancellationCheck for Cancel {
    fn check(&self) -> Result<(), String> {
        if self.0.load(Ordering::SeqCst) {
            Err("cancelled".into())
        } else {
            Ok(())
        }
    }
}

/// A source reading through `git` with `deadline` for each git step and
/// `cancellation`.
fn source(
    home: &Path,
    server: &MockServer,
    git: Arc<dyn GitRunner>,
    credential: Option<ResolvedCredential>,
    deadline: Duration,
    cancellation: Arc<dyn CancellationCheck + Send>,
) -> BlobSource {
    BlobSource::new(Arc::new(GitSetup {
        config: GitConfig::persistent(home),
        endpoint: mock_endpoint(server),
        git_limiter: octocode_github::GitLimiter::new(
            octocode_github::GitHubBudget::relaxed(),
            None,
        ),
        credential,
        cancellation,
        deadline: Instant::now() + deadline,
        path_policy: PathPolicy::new(PathPolicyConfig::default()).expect("policy"),
        git,
    }))
    .expect("a loopback endpoint has a git URL")
}

fn context() -> RequestContext {
    RequestContext::new(
        RequestBudget::with_timeout(Duration::from_secs(20), READ_LIMIT),
        None,
    )
}

/// Everything a call leaves, comparable across homes.
#[derive(Debug, PartialEq)]
struct Landed {
    outcome: Value,
    /// Path under the materialize root to bytes (and the executable bit).
    files: BTreeMap<String, (Vec<u8>, bool)>,
}

fn outcome_of(outcome: &MaterializeOutcome) -> Value {
    let mut location = outcome.location.clone();
    location.as_object_mut().unwrap().remove("path");
    json!({
        "location": location,
        "next": outcome.next_offset,
        "warnings": outcome.warnings,
        "gap": outcome.gap,
        "hint": outcome.hint,
    })
}

fn tree_of(home: &Path) -> BTreeMap<String, (Vec<u8>, bool)> {
    fn walk(root: &Path, at: &Path, into: &mut BTreeMap<String, (Vec<u8>, bool)>) {
        let Ok(read) = std::fs::read_dir(at) else {
            return;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, into);
            } else if path.extension().is_none_or(|extension| extension != "lock") {
                #[cfg(unix)]
                let executable = std::os::unix::fs::PermissionsExt::mode(
                    &std::fs::metadata(&path).unwrap().permissions(),
                ) & 0o111
                    != 0;
                #[cfg(not(unix))]
                let executable = false;
                into.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    (std::fs::read(&path).unwrap(), executable),
                );
            }
        }
    }
    let root = crate::gh_structure::materialize::materialize_root(home);
    let mut files = BTreeMap::new();
    walk(&root, &root, &mut files);
    files
}

async fn materialize(
    server: &MockServer,
    remote: &Remote,
    home: &Path,
    blobs: Option<&BlobSource>,
    offset: usize,
) -> Result<Option<MaterializeOutcome>, octocode_github::ProviderError> {
    let snapshot = Snapshot {
        owner: "octo",
        repo: "demo",
        commit: &remote.commit,
        root: home,
        blobs,
    };
    materialize_tree(
        &one_shot_provider(server),
        &snapshot,
        &remote.entries,
        offset,
        "",
        &context(),
    )
    .await
}

async fn snapshot_of(
    server: &MockServer,
    remote: &Remote,
    home: &Path,
    blobs: Option<&BlobSource>,
) -> Landed {
    let outcome = materialize(server, remote, home, blobs, 0)
        .await
        .expect("materialized")
        .expect("entries");
    Landed {
        outcome: outcome_of(&outcome),
        files: tree_of(home),
    }
}

fn contents_requests(requests: &[wiremock::Request]) -> usize {
    requests
        .iter()
        .filter(|request| request.url.path().contains("/contents/"))
        .count()
}

fn git_source(home: &Path, server: &MockServer, git: &Arc<FixtureGit>) -> BlobSource {
    source(
        home,
        server,
        git.clone(),
        None,
        Duration::from_secs(30),
        Arc::new(NeverCancel),
    )
}

/// The store directories left under the home.
fn leftover_stores(home: &Path) -> Vec<String> {
    std::fs::read_dir(home.join("tmp").join("materialize-git"))
        .map(|read| {
            read.flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn blob_source_matches_rest_outcomes() {
    let remote = Remote::new(&mixed());
    let server = MockServer::start().await;
    remote.serve(&server).await;
    let rest_home = Fixture::new();
    let rest = snapshot_of(&server, &remote, &rest_home.0, None).await;
    let rest_requests = server.received_requests().await.unwrap();
    assert!(
        contents_requests(&rest_requests) > 0,
        "the REST path reads per file"
    );

    let git_home = Fixture::new();
    let fixture = Arc::new(FixtureGit::new(&remote));
    let blobs = git_source(&git_home.0, &server, &fixture);
    let through_git = snapshot_of(&server, &remote, &git_home.0, Some(&blobs)).await;

    assert_eq!(through_git, rest);
    // The mix was real: written, withheld, oversized and binary all happened.
    let location = &rest.outcome["location"];
    assert!(
        location["skipped"]
            .as_array()
            .is_some_and(|skipped| skipped.len() >= 5)
    );
    assert!(rest.files.keys().any(|name| name.ends_with("src/a.rs")));
    assert!(
        rest.files
            .keys()
            .any(|name| name.ends_with("manifest.json"))
    );
    assert!(
        !rest
            .files
            .keys()
            .any(|name| name.ends_with("deploy/id_rsa"))
    );
    // Neither path marks a file executable (the bit of `tool.sh` is dropped).
    assert!(rest.files.values().all(|(_, executable)| !executable));
    assert!(!fixture.fetches().is_empty(), "git served the bytes");
    // Every REST read after the listing was avoided.
    let all = server.received_requests().await.unwrap();
    assert_eq!(
        contents_requests(&all),
        contents_requests(&rest_requests),
        "the git run sent no per-file request"
    );
}

#[tokio::test]
async fn git_path_uses_no_per_file_requests_and_reuse_uses_none() {
    let remote = Remote::new(&mixed());
    let server = MockServer::start().await;
    remote.serve(&server).await;
    let home = Fixture::new();
    let fixture = Arc::new(FixtureGit::new(&remote));
    let blobs = git_source(&home.0, &server, &fixture);
    materialize(&server, &remote, &home.0, Some(&blobs), 0)
        .await
        .unwrap();
    assert_eq!(
        contents_requests(&server.received_requests().await.unwrap()),
        0
    );
    let fetches = fixture.fetches();
    assert_eq!(fetches.len(), 1, "one wave, one transfer");
    let wanted = String::from_utf8(fetches[0].stdin.clone().unwrap()).unwrap();
    let wanted: Vec<&str> = wanted.lines().collect();
    let oid_of = |name: &str| {
        remote
            .entries
            .iter()
            .find(|entry| entry.path == name)
            .unwrap()
            .oid
            .clone()
            .unwrap()
    };
    assert!(wanted.contains(&oid_of("src/a.rs").as_str()));
    for never in [".env", "deploy/id_rsa", "pkg/.npmrc", "big.txt"] {
        assert!(
            !wanted.contains(&oid_of(never).as_str()),
            "{never} is withheld or oversized before any transfer"
        );
    }
    let reads: Vec<Call> = fixture
        .calls()
        .into_iter()
        .filter(|call| call.label == "read fetched blobs")
        .collect();
    assert_eq!(reads.len(), 1);

    // A repeat finds every written file in the snapshot: only the binary,
    // which was never written, is fetched again (as the REST path reads it
    // again).
    materialize(&server, &remote, &home.0, Some(&blobs), 0)
        .await
        .unwrap();
    let fetches = fixture.fetches();
    assert_eq!(fetches.len(), 2);
    assert_eq!(
        String::from_utf8(fetches[1].stdin.clone().unwrap()).unwrap(),
        format!("{}\n", oid_of("src/data.bin")),
        "Kept files are not fetched again"
    );
}

/// `ls-tree -l`, `cat-file -s` and `--batch-check` fetch the blobs a partial
/// store lacks one at a time; the source asks git for none of them.
#[tokio::test]
async fn no_size_or_listing_queries_reach_the_store() {
    let remote = Remote::new(&mixed());
    let server = MockServer::start().await;
    remote.serve(&server).await;
    let home = Fixture::new();
    let fixture = Arc::new(FixtureGit::new(&remote));
    let blobs = git_source(&home.0, &server, &fixture);
    materialize(&server, &remote, &home.0, Some(&blobs), 0)
        .await
        .unwrap();
    for call in fixture.calls() {
        for argument in &call.args {
            assert!(
                !["ls-tree", "-s", "-l", "--batch-check", "--long", "-t"]
                    .contains(&argument.as_str()),
                "{call:?}"
            );
        }
        if call.args.iter().any(|argument| argument == "cat-file") {
            assert!(call.args.iter().any(|argument| argument == "--batch"));
        }
    }
}

#[tokio::test]
async fn git_absent_falls_back_to_rest() {
    let remote = Remote::new(&mixed());
    let server = MockServer::start().await;
    remote.serve(&server).await;
    let rest_home = Fixture::new();
    let rest = snapshot_of(&server, &remote, &rest_home.0, None).await;
    let before = contents_requests(&server.received_requests().await.unwrap());

    let home = Fixture::new();
    struct Absent;
    impl GitRunner for Absent {
        fn run(&self, r: &GitRunRequest<'_>, c: &GitRunControl<'_>) -> Result<GitOutput, GitError> {
            SystemGit::new("octocode-no-such-git").run(r, c)
        }
        fn run_nul_records(
            &self,
            r: &GitRunRequest<'_>,
            c: &GitRunControl<'_>,
            record: &mut (dyn FnMut(&[u8]) -> Result<(), GitError> + Send),
        ) -> Result<GitOutput, GitError> {
            SystemGit::new("octocode-no-such-git").run_nul_records(r, c, record)
        }
        fn run_stream(
            &self,
            r: &GitRunRequest<'_>,
            c: &GitRunControl<'_>,
            sink: &mut (dyn FnMut(&[u8]) -> Result<(), GitError> + Send),
        ) -> Result<GitOutput, GitError> {
            SystemGit::new("octocode-no-such-git").run_stream(r, c, sink)
        }
    }
    let blobs = source(
        &home.0,
        &server,
        Arc::new(Absent),
        None,
        Duration::from_secs(30),
        Arc::new(NeverCancel),
    );
    let result = snapshot_of(&server, &remote, &home.0, Some(&blobs)).await;
    assert_eq!(result, rest);
    let after = contents_requests(&server.received_requests().await.unwrap());
    assert_eq!(after - before, before, "every file was read over REST");
    assert!(leftover_stores(&home.0).is_empty());
}

fn many(count: usize) -> Vec<(String, Vec<u8>)> {
    (0..count)
        .map(|n| (format!("f{n:02}.txt"), format!("{n:0>400}").into_bytes()))
        .collect()
}

fn many_remote(count: usize) -> Remote {
    let files = many(count);
    let mut all: Vec<(&str, Vec<u8>)> = files
        .iter()
        .map(|(name, body)| (name.as_str(), body.clone()))
        .collect();
    all.push(("tool.sh", b"#!/bin/sh\n".to_vec()));
    Remote::new(&all)
}

/// A server that refuses the ids refuses the wave, and the call reads the
/// rest over REST without asking git again.
#[tokio::test]
async fn refused_fetch_falls_back_for_the_wave_and_later_waves_skip_git() {
    let remote = many_remote(9);
    let server = MockServer::start().await;
    remote.serve(&server).await;
    let rest_home = Fixture::new();
    let rest = snapshot_of(&server, &remote, &rest_home.0, None).await;

    let home = Fixture::new();
    let mut fixture = FixtureGit::new(&remote);
    fixture.refuse_fetch = true;
    let fixture = Arc::new(fixture);
    let blobs = git_source(&home.0, &server, &fixture);
    let result = snapshot_of(&server, &remote, &home.0, Some(&blobs)).await;
    assert_eq!(result, rest);
    assert_eq!(fixture.fetches().len(), 1, "git is not asked again");
    assert!(leftover_stores(&home.0).is_empty());
}

/// Waves are the listed bytes up to the read limit: each is one transfer, and
/// the walk ends as the one-read-per-file walk does.
#[tokio::test]
async fn a_long_listing_is_fetched_in_waves_of_the_read_limit() {
    let remote = many_remote(30);
    let server = MockServer::start().await;
    remote.serve(&server).await;
    let rest_home = Fixture::new();
    let rest = snapshot_of(&server, &remote, &rest_home.0, None).await;

    let home = Fixture::new();
    let fixture = Arc::new(FixtureGit::new(&remote));
    let blobs = git_source(&home.0, &server, &fixture);
    let result = snapshot_of(&server, &remote, &home.0, Some(&blobs)).await;
    assert_eq!(result, rest);
    // 400-byte files, 1024-byte waves: three files make a wave.
    let fetches = fixture.fetches();
    assert!(fetches.len() > 1, "{} transfers", fetches.len());
    let wave = |call: &Call| {
        String::from_utf8_lossy(call.stdin.as_ref().unwrap())
            .lines()
            .count()
    };
    assert!(
        fetches.iter().all(|call| wave(call) <= 3),
        "bounded by the limit"
    );
}

/// A blob longer than the listed size said is oversized, not written.
#[tokio::test]
async fn a_blob_past_the_read_limit_is_oversized_whatever_the_listing_said() {
    let remote = Remote::new(&[
        ("grown.txt", vec![b'g'; READ_LIMIT * 3]),
        ("tool.sh", b"#!/bin/sh\n".to_vec()),
    ]);
    let mut entries = remote.entries.clone();
    for entry in &mut entries {
        if entry.path == "grown.txt" {
            entry.size = Some(10);
        }
    }
    let remote = Remote { entries, ..remote };
    let server = MockServer::start().await;
    let home = Fixture::new();
    let fixture = Arc::new(FixtureGit::new(&remote));
    let blobs = git_source(&home.0, &server, &fixture);
    let outcome = materialize(&server, &remote, &home.0, Some(&blobs), 0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        outcome.location["skipped"],
        json_fixture(r#"["grown.txt"]"#)
    );
    assert!(outcome.gap, "an oversized text file leaves a gap");
    assert!(
        !tree_of(&home.0)
            .keys()
            .any(|name| name.ends_with("grown.txt"))
    );
    assert_eq!(
        contents_requests(&server.received_requests().await.unwrap()),
        0
    );
}

/// Time runs out between waves: the call keeps what it wrote, resumes at the
/// first file of the wave that stopped, and the two calls together leave what
/// one call would.
#[tokio::test]
async fn a_stopped_wave_resumes_at_its_first_entry() {
    let remote = many_remote(9);
    let server = MockServer::start().await;
    remote.serve(&server).await;
    let whole_home = Fixture::new();
    let whole = snapshot_of(&server, &remote, &whole_home.0, None).await;

    let home = Fixture::new();
    let mut slow = FixtureGit::new(&remote);
    // The second transfer starts after this call's git deadline.
    slow.before_fetch = Some(Box::new(|number| {
        if number == 2 {
            std::thread::sleep(Duration::from_millis(1500));
        }
    }));
    let slow = Arc::new(slow);
    let short = source(
        &home.0,
        &server,
        slow.clone(),
        None,
        Duration::from_millis(1200),
        Arc::new(NeverCancel),
    );
    let first = materialize(&server, &remote, &home.0, Some(&short), 0)
        .await
        .unwrap()
        .unwrap();
    let resume = first.next_offset.expect("the second wave was stopped");
    assert_eq!(resume, 3, "three 400-byte files make the first wave");
    assert!(
        first
            .warnings
            .iter()
            .any(|warning| warning.starts_with("Stopped at"))
    );

    let fixture = Arc::new(FixtureGit::new(&remote));
    let blobs = git_source(&home.0, &server, &fixture);
    let last = materialize(&server, &remote, &home.0, Some(&blobs), resume)
        .await
        .unwrap()
        .unwrap();
    assert!(last.next_offset.is_none());
    assert_eq!(tree_of(&home.0), whole.files);
    assert_eq!(
        outcome_of(&last)["location"]["coverage"],
        json_fixture(r#"{"state": "complete"}"#)
    );
}

/// The throwaway store is gone when the call succeeded, when it failed and
/// when it was cancelled; a dead process's store is swept, a live one's stays.
#[tokio::test]
async fn the_throwaway_store_never_outlives_the_call() {
    let remote = many_remote(6);
    let server = MockServer::start().await;
    remote.serve(&server).await;

    let home = Fixture::new();
    let fixture = Arc::new(FixtureGit::new(&remote));
    let blobs = git_source(&home.0, &server, &fixture);
    materialize(&server, &remote, &home.0, Some(&blobs), 0)
        .await
        .unwrap();
    assert!(leftover_stores(&home.0).is_empty(), "after success");

    // Cancelled while a transfer runs.
    let home = Fixture::new();
    let flag = Arc::new(AtomicBool::new(false));
    let mut cancelling = FixtureGit::new(&remote);
    let raised = flag.clone();
    cancelling.before_fetch = Some(Box::new(move |_| raised.store(true, Ordering::SeqCst)));
    let blobs = source(
        &home.0,
        &server,
        Arc::new(cancelling),
        None,
        Duration::from_secs(30),
        Arc::new(Cancel(flag)),
    );
    let error = materialize(&server, &remote, &home.0, Some(&blobs), 0)
        .await
        .err()
        .expect("a cancelled call fails");
    assert_eq!(error.kind, octocode_github::ProviderErrorKind::Cancelled);
    assert!(leftover_stores(&home.0).is_empty(), "after cancel");

    // A killed call's store: swept by the next call, unless its process lives.
    let home = Fixture::new();
    let parent = home.0.join("tmp").join("materialize-git");
    let nonce = "0123456789abcdef0123456789abcdef";
    let dead = format!("2000000000-{nonce}");
    let live = format!("{}-{nonce}", parent_pid());
    for name in [&dead, &live] {
        std::fs::create_dir_all(parent.join(name)).unwrap();
    }
    let fixture = Arc::new(FixtureGit::new(&remote));
    let blobs = git_source(&home.0, &server, &fixture);
    materialize(&server, &remote, &home.0, Some(&blobs), 0)
        .await
        .unwrap();
    assert_eq!(leftover_stores(&home.0), vec![live]);
}

#[cfg(unix)]
fn parent_pid() -> u32 {
    std::os::unix::process::parent_id()
}
#[cfg(not(unix))]
fn parent_pid() -> u32 {
    std::process::id()
}

/// The credential reaches git through its environment only: never an
/// argument, a URL or stdin, and only for the transfer.
#[tokio::test]
async fn the_token_is_never_in_an_argument_or_a_url() {
    let remote = many_remote(3);
    let server = MockServer::start().await;
    remote.serve(&server).await;
    let home = Fixture::new();
    let fixture = Arc::new(FixtureGit::new(&remote));
    let blobs = source(
        &home.0,
        &server,
        fixture.clone(),
        Some(ResolvedCredential::new(TOKEN, CredentialSource::Override)),
        Duration::from_secs(30),
        Arc::new(NeverCancel),
    );
    materialize(&server, &remote, &home.0, Some(&blobs), 0)
        .await
        .unwrap();
    let calls = fixture.calls();
    assert!(calls.iter().any(|call| call.label == "fetch blobs"));
    for call in &calls {
        let spoken = format!("{:?} {:?}", call.args, call.authorization_url);
        assert!(!spoken.contains(TOKEN), "{call:?}");
        if let Some(stdin) = &call.stdin {
            assert!(!String::from_utf8_lossy(stdin).contains(TOKEN));
        }
        // Only the network step names a remote the credential may answer for.
        assert_eq!(
            call.authorization_url.is_some(),
            call.label == "fetch blobs",
            "{call:?}"
        );
        if call.label == "fetch blobs" {
            let url = call.authorization_url.as_deref().unwrap();
            assert!(url.ends_with("/octo/demo.git"), "{url}");
            assert!(!url.contains('@'));
            // It travels beside the request, to the askpass helper's environment.
            assert_eq!(call.authorization.as_deref(), Some(TOKEN));
        }
    }
}

/// Links and submodules are not listed as files, so no path writes them; a
/// listed file's blob id is its own, never a tree's.
#[test]
fn symlinks_and_submodules_are_not_files_and_directories_carry_no_blob_id() {
    use octocode_github::{TreeEntry as Raw, TreeResponse};
    let raw = |path: &str, kind: &str, mode: &str| Raw {
        path: path.into(),
        kind: kind.into(),
        mode: Some(mode.into()),
        size: Some(1),
        sha: Some(format!("{:0>40}", path.len())),
        url: None,
    };
    let tree = TreeResponse {
        sha: "0".repeat(40),
        truncated: false,
        tree: vec![
            raw("dir", "tree", "040000"),
            raw("dir/file", "blob", "100755"),
            raw("link", "blob", "120000"),
            raw("sub", "commit", "160000"),
        ],
    };
    let listing = crate::gh_structure::Listing {
        owner: "octo",
        repo: "demo",
        branch: "main",
        root: "",
        max_depth: 5,
    };
    let traversal = crate::gh_structure::traversal_from_git_tree(tree, &listing);
    let by_path: BTreeMap<_, _> = traversal
        .entries
        .iter()
        .map(|entry| (entry.path.as_str(), entry.oid.is_some()))
        .collect();
    assert_eq!(
        by_path,
        BTreeMap::from([("dir", false), ("dir/file", true), ("link", true)])
    );
    assert_eq!(
        traversal
            .omitted
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["submodule"]
    );
}
