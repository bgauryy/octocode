//! A file read finds the symlink on its path before any of its bytes are
//! asked for, against a repository answered as GitHub answers it
//! ([`octocode_test_support::mock_http::GitRepository`]): a link to a file is read by its
//! target's name (GitHub would serve the target's bytes under the link's
//! name), a target the caller withholds is never asked for, the leaf's parent
//! tree is the one check per folder, and a check that cannot be made serves
//! nothing.
use super::*;
use octocode_test_support::mock_http::GitRepository;
use std::collections::HashMap;
use std::sync::Mutex;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const API: &str = "/api/v3/repos/a/b";

#[derive(Default)]
pub(super) struct Held(Mutex<HashMap<(String, String), crate::SavedAnswer>>);

impl crate::SavedAnswers for Held {
    fn load(&self, scope: &str, request: &str) -> Option<crate::SavedAnswer> {
        let map = self.0.lock().unwrap();
        map.get(&(scope.to_owned(), request.to_owned())).cloned()
    }
    fn save(&self, scope: &str, request: &str, saved: crate::SavedAnswer) {
        let mut map = self.0.lock().unwrap();
        map.insert((scope.to_owned(), request.to_owned()), saved);
    }
    fn forget(&self, scope: &str, request: &str) {
        self.0
            .lock()
            .unwrap()
            .remove(&(scope.to_owned(), request.to_owned()));
    }
}

impl Held {
    /// Whether any kept answer's body holds `text`.
    fn holds(&self, text: &str) -> bool {
        self.0
            .lock()
            .unwrap()
            .values()
            .any(|saved| saved.answer.body.as_str().contains(text))
    }
}

pub(super) async fn keeping_provider(server: &MockServer) -> (GitHubProvider, Arc<Held>) {
    let mut transport = GitHubTransport::new(
        mock_endpoint(server),
        RetryPolicy {
            max_attempts: 2,
            base_delay: Duration::from_millis(1),
        },
        GitHubBudget::relaxed(),
    )
    .expect("transport");
    let held = Arc::new(Held::default());
    transport.set_saved_answers(Some(held.clone()));
    (GitHubProvider::new(transport), held)
}

fn read_at(path: &str) -> ContentRequest {
    ContentRequest {
        owner: "a".into(),
        repo: "b".into(),
        path: path.into(),
        reference: Some(COMMIT.into()),
    }
}

fn context() -> RequestContext {
    with_token("secret", Duration::from_secs(5), 1 << 20)
}

/// Requests to `route` under the repository, as the client spelled it.
async fn asked(server: &MockServer, route: &str) -> usize {
    let wanted = format!("{API}{route}");
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == wanted)
        .count()
}

/// Requests whose path starts with `prefix` under the repository.
async fn asked_under(server: &MockServer, prefix: &str) -> usize {
    let wanted = format!("{API}{prefix}");
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path().starts_with(&wanted))
        .count()
}

fn never_withheld(_: &str) -> bool {
    false
}

/// The chatbot_ner shape: `config.example` is a link to `.env.example`, whose
/// contents read GitHub answers with the target's bytes under the link's name.
fn env_link() -> GitRepository {
    GitRepository::new()
        .file(".env.example", "SECRET_KEY=never-on-disk-value\n")
        .link("config.example", ".env.example")
        .file("README.md", "readme\n")
}

/// A link to a withheld file: its target is learned from the link's own
/// blob, and neither the link's contents nor the target's are asked for or
/// kept. A refusing read names the target.
#[tokio::test]
async fn a_withheld_link_target_is_never_requested_or_kept() {
    let server = MockServer::builder().start().await;
    env_link().mount(&server, "a", "b", COMMIT).await;
    let (provider, held) = keeping_provider(&server).await;
    let withheld = provider
        .get_file_bytes_following_links(&read_at("config.example"), &context(), &|target| {
            target == ".env.example"
        })
        .await
        .expect("checked");
    assert_eq!(withheld, Err(".env.example".to_owned()));
    let refused = provider
        .get_file_bytes(&read_at("config.example"), &context())
        .await
        .expect_err("a link");
    assert!(
        refused.message.contains(".env.example"),
        "{}",
        refused.message
    );
    assert_eq!(asked_under(&server, "/contents").await, 0);
    assert_eq!(
        asked(
            &server,
            &format!("/git/blobs/{}", GitRepository::blob_sha("config.example"))
        )
        .await,
        1,
        "the link's blob is one shared answer"
    );
    assert!(!held.holds("never-on-disk-value"));
    assert!(!held.holds(&STANDARD.encode("SECRET_KEY")[..8]));
}

/// The cloudbreak shape: `flow/CLAUDE.md` links to `AGENTS.md` beside it. A
/// following read serves the target's text by the target's name; one tree of
/// the folder answers both paths.
#[tokio::test]
async fn an_ordinary_file_link_is_followed_by_its_target_name() {
    let server = MockServer::builder().start().await;
    GitRepository::new()
        .file("flow/AGENTS.md", "# Agents\nguidance\n")
        .link("flow/CLAUDE.md", "AGENTS.md")
        .mount(&server, "a", "b", COMMIT)
        .await;
    let (provider, _) = keeping_provider(&server).await;
    let (read, target) = provider
        .get_file_bytes_following_links(&read_at("flow/CLAUDE.md"), &context(), &never_withheld)
        .await
        .expect("read")
        .expect("not withheld");
    assert_eq!(read.bytes, &b"# Agents\nguidance\n"[..]);
    assert_eq!(target.as_deref(), Some("flow/AGENTS.md"));
    assert_eq!(asked(&server, "/contents/flow%2FCLAUDE.md").await, 0);
    assert_eq!(asked(&server, "/contents/flow%2FAGENTS.md").await, 1);
    assert_eq!(
        asked(&server, &format!("/git/trees/{COMMIT}:flow")).await,
        1
    );
    assert_eq!(asked(&server, &format!("/git/trees/{COMMIT}")).await, 0);
}

/// The linux shape: a file under a symlinked directory. GitHub answers no
/// tree through the link, so the directories above are asked, deepest first,
/// until one names the link; the read then goes to the target's file.
#[tokio::test]
async fn a_file_under_a_symlinked_directory_is_found_by_walking_up() {
    let server = MockServer::builder().start().await;
    GitRepository::new()
        .file("include/dt-bindings/gpio/gpio.h", "#define GPIO 1\n")
        .file("scripts/dtc/include-prefixes/README", "prefixes\n")
        .link(
            "scripts/dtc/include-prefixes/dt-bindings",
            "../../../include/dt-bindings",
        )
        .mount(&server, "a", "b", COMMIT)
        .await;
    let (provider, _) = keeping_provider(&server).await;
    let path = "scripts/dtc/include-prefixes/dt-bindings/gpio/gpio.h";
    let (read, target) = provider
        .get_file_bytes_following_links(&read_at(path), &context(), &never_withheld)
        .await
        .expect("read")
        .expect("not withheld");
    assert_eq!(read.bytes, &b"#define GPIO 1\n"[..]);
    assert_eq!(target.as_deref(), Some("include/dt-bindings/gpio/gpio.h"));
    let refused = provider
        .get_file_bytes(&read_at(path), &context())
        .await
        .expect_err("a link on the path");
    assert!(
        refused.message.contains("include/dt-bindings"),
        "{}",
        refused.message
    );
    // The walk stops at the first directory whose tree answers.
    assert_eq!(
        asked(&server, &format!("/git/trees/{COMMIT}:scripts%2Fdtc")).await,
        0
    );
    assert_eq!(asked(&server, &format!("/git/trees/{COMMIT}")).await, 0);
    assert_eq!(asked_under(&server, "/contents/scripts").await, 0);
}

/// Twenty files of one folder: its tree is asked once per commit and no tree
/// above it is asked at all.
#[tokio::test]
async fn files_of_one_folder_share_its_tree() {
    let server = MockServer::builder().start().await;
    let names = (0..20).map(|n| format!("f{n}.md")).collect::<Vec<_>>();
    let mut repository = GitRepository::new();
    for name in &names {
        repository = repository.file(&format!("deep/er/docs/{name}"), "text\n");
    }
    repository.mount(&server, "a", "b", COMMIT).await;
    let (provider, _) = keeping_provider(&server).await;
    let context = context();
    for name in &names {
        provider
            .get_file_bytes(&read_at(&format!("deep/er/docs/{name}")), &context)
            .await
            .expect("read");
    }
    assert_eq!(asked_under(&server, "/git/trees/").await, 1);
    assert_eq!(
        asked(&server, &format!("/git/trees/{COMMIT}:deep%2Fer%2Fdocs")).await,
        1
    );
}

/// A recursive tree of the commit already held answers the whole check: no
/// tree is asked for any directory, and a link is found by its mode.
#[tokio::test]
async fn a_held_tree_answers_the_check_with_no_tree_request() {
    let server = MockServer::builder().start().await;
    GitRepository::new()
        .file("docs/a.md", "a\n")
        .file("docs/b.md", "b\n")
        .file("docs/.env", "TOKEN=x\n")
        .link("docs/env", ".env")
        .mount(&server, "a", "b", COMMIT)
        .await;
    let (provider, _) = keeping_provider(&server).await;
    let context = context();
    provider
        .transport
        .get_tree(
            &crate::TreeRequest {
                owner: "a".into(),
                repo: "b".into(),
                reference: COMMIT.into(),
                recursive: true,
            },
            &context,
        )
        .await
        .expect("tree");
    for name in ["docs/a.md", "docs/b.md"] {
        provider
            .get_file_bytes(&read_at(name), &context)
            .await
            .expect("read");
    }
    let refused = provider
        .get_file_bytes(&read_at("docs/env"), &context)
        .await
        .expect_err("a link");
    assert!(refused.message.contains("docs/.env"), "{}", refused.message);
    // Only the held recursive tree was ever asked.
    assert_eq!(asked_under(&server, "/git/trees/").await, 1);
}

/// A directory of a thousand entries is checked whole by its git tree, which
/// has no entry cap: a link listed last is still found.
#[tokio::test]
async fn a_directory_of_a_thousand_entries_is_checked_whole() {
    let server = MockServer::builder().start().await;
    let mut repository = GitRepository::new().link("zz-link", "f0001");
    for n in 0..1000 {
        repository = repository.file(&format!("f{n:04}"), "x\n");
    }
    repository.mount(&server, "a", "b", COMMIT).await;
    let (provider, _) = keeping_provider(&server).await;
    let refused = provider
        .get_file_bytes(&read_at("zz-link"), &context())
        .await
        .expect_err("a link past a thousand entries");
    assert!(refused.message.contains("f0001"), "{}", refused.message);
}

/// A directory too large for even its git tree cannot be checked: the read
/// fails and no bytes are asked for.
#[tokio::test]
async fn an_unchecked_directory_serves_no_bytes() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        format!("{API}/git/trees/{COMMIT}"),
        200,
        serde_json::json!({"sha": COMMIT, "truncated": true, "tree": []}),
    )
    .await;
    mount_json(
        &server,
        format!("{API}/contents/zz-file"),
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("bytes\n")}),
    )
    .await;
    let (provider, _) = keeping_provider(&server).await;
    let error = provider
        .get_file_bytes(&read_at("zz-file"), &context())
        .await
        .expect_err("not checkable");
    assert_eq!(error.kind, ProviderErrorKind::Unavailable);
    assert_eq!(asked(&server, "/contents/zz-file").await, 0);
}

/// A link check GitHub answers with a 404 or 422 checks nothing: the read
/// asks GitHub only whether the path exists (a HEAD, which GitHub answers
/// with the status a GET would have and no body), so the bytes no link check
/// covered are never downloaded. A path that exists is refused and kept
/// nowhere; only a 404 is a missing file.
#[tokio::test]
async fn a_tree_404_or_422_fails_the_read_closed() {
    for status in [404, 422] {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(wiremock::matchers::path_regex("/git/trees/"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        mount_json(
            &server,
            format!("{API}/contents/config.example"),
            200,
            serde_json::json!({"type":"file","encoding":"base64",
                "content":STANDARD.encode("SECRET_KEY=unchecked-target-value\n")}),
        )
        .await;
        Mock::given(method("HEAD"))
            .and(wiremock::matchers::path(format!(
                "{API}/contents/config.example"
            )))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let (provider, held) = keeping_provider(&server).await;
        let error = provider
            .get_file_bytes_following_links(&read_at("config.example"), &context(), &never_withheld)
            .await
            .expect_err("unchecked");
        assert_eq!(error.kind, ProviderErrorKind::Unavailable, "{status}");
        assert!(
            error.message.contains("config.example"),
            "{}",
            error.message
        );
        assert!(!held.holds(&STANDARD.encode("SECRET_KEY=unchecked")[..12]));
        let downloads = server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .filter(|request| {
                request.method.as_str() == "GET"
                    && request.url.path().starts_with(&format!("{API}/contents/"))
            })
            .count();
        assert_eq!(downloads, 0, "{status}: no body was asked for");
        let missing = provider
            .get_file_bytes(&read_at("nothing.txt"), &context())
            .await
            .expect_err("missing");
        assert_eq!(missing.kind, ProviderErrorKind::NotFound, "{status}");
    }
}

/// A path the commit's trees do not list is GitHub's not-found, after the
/// one tree of its parent.
#[tokio::test]
async fn a_path_the_tree_lacks_is_not_found() {
    let server = MockServer::builder().start().await;
    env_link().mount(&server, "a", "b", COMMIT).await;
    let (provider, _) = keeping_provider(&server).await;
    let missing = provider
        .get_file_bytes(&read_at("src/missing.rs"), &context())
        .await
        .expect_err("missing");
    assert_eq!(missing.kind, ProviderErrorKind::NotFound);
    assert_eq!(asked(&server, &format!("/git/trees/{COMMIT}:src")).await, 1);
    assert_eq!(asked(&server, &format!("/git/trees/{COMMIT}")).await, 1);
}

/// A link to a path outside the repository, absolute or climbing past its
/// root, is refused naming the target it holds; a chain that loops is
/// refused after a few links, naming the last one.
#[tokio::test]
async fn links_out_of_the_repository_or_in_a_loop_are_refused_by_target() {
    let server = MockServer::builder().start().await;
    GitRepository::new()
        .link("key", "/root/id_rsa")
        .link("gimmedakeys", "../.ssh/id_rsa")
        .link("ping", "pong")
        .link("pong", "ping")
        .mount(&server, "a", "b", COMMIT)
        .await;
    let (provider, _) = keeping_provider(&server).await;
    for (link, target) in [
        ("key", "/root/id_rsa"),
        ("gimmedakeys", "../.ssh/id_rsa"),
        ("ping", "ping"),
    ] {
        let refused = provider
            .get_file_bytes_following_links(&read_at(link), &context(), &never_withheld)
            .await
            .expect_err("refused");
        assert!(refused.message.contains(target), "{}", refused.message);
    }
}
