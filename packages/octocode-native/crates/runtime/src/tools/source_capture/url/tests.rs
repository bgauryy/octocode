use super::*;
use crate::config::{ConfigInput, FileInput, RuntimeSurface};
use crate::policy::path::PathPolicyConfig;
use crate::runtime::{Channel, ToolRuntime};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

/// A canned reply: status, Location, Content-Type, body.
type Canned = (u16, Option<&'static str>, Option<&'static str>, Vec<u8>);

/// Replies by URL (fragment excluded); records every requested URL.
#[derive(Default)]
struct Fake {
    replies: HashMap<String, Canned>,
    requested: RefCell<Vec<String>>,
}

impl Fake {
    fn page(mut self, url: &str, content_type: &'static str, body: &str) -> Self {
        self.replies.insert(
            url.into(),
            (200, None, Some(content_type), body.as_bytes().to_vec()),
        );
        self
    }

    fn bytes(mut self, url: &str, content_type: Option<&'static str>, body: Vec<u8>) -> Self {
        self.replies
            .insert(url.into(), (200, None, content_type, body));
        self
    }

    fn status(mut self, url: &str, status: u16, location: Option<&'static str>) -> Self {
        self.replies
            .insert(url.into(), (status, location, None, Vec::new()));
        self
    }
}

impl Fetch for Fake {
    fn get(&self, url: &Url) -> Result<Reply, ClassificationError> {
        let mut bare = url.clone();
        bare.set_fragment(None);
        self.requested.borrow_mut().push(bare.to_string());
        let (status, location, content_type, body) = self
            .replies
            .get(bare.as_str())
            .cloned()
            .unwrap_or((404, None, None, Vec::new()));
        Ok(Reply {
            status,
            location: location.map(str::to_owned),
            content_type: content_type.map(str::to_owned),
            body,
        })
    }
}

const PAGE: &str = r#"<html><body><nav>Home</nav>
<h1>Guide</h1><p>Intro.</p>
<h2 id="rotation">Rotation</h2><p>Update the configuration, then restart.</p>
<h3>Check</h3><p>Run status.</p>
<h2 id="faq">FAQ</h2><p>None.</p></body></html>"#;

#[test]
fn image_capture_keeps_binary_bytes_and_final_origin() {
    let bytes = vec![0x89, b'P', b'N', b'G', 0, 0xff, 0xfe];
    let fake = Fake::default()
        .status("https://example.org/image", 302, Some("/actual.png"))
        .bytes(
            "https://example.org/actual.png",
            Some("image/png"),
            bytes.clone(),
        );
    let (captured, source) =
        select_image(&fake, parse("https://example.org/image").unwrap()).unwrap();
    assert_eq!(captured, bytes);
    assert_eq!(source["url"], "https://example.org/actual.png");
    assert_eq!(fake.requested.borrow().len(), 2);
}

#[test]
fn an_image_source_never_follows_embedded_images_or_unsupported_media() {
    for (media, body) in [
        ("text/html", b"<img src='/nested.png'>".as_slice()),
        ("image/webp", b"RIFFWEBP".as_slice()),
    ] {
        let fake = Fake::default().bytes("https://example.org/image", Some(media), body.to_vec());
        let error = select_image(&fake, parse("https://example.org/image").unwrap()).unwrap_err();
        assert_eq!(error.code, "classificationContextUnavailable");
        assert_eq!(fake.requested.borrow().len(), 1);
    }
}

#[test]
fn image_fragments_are_rejected_at_origin_and_after_redirect() {
    let fake = Fake::default()
        .status(
            "https://example.org/image",
            302,
            Some("/actual.png#xywh=0,0,1,1"),
        )
        .bytes(
            "https://example.org/actual.png",
            Some("image/png"),
            vec![1, 2, 3],
        );
    assert_eq!(
        select_image(&fake, parse("https://example.org/image#crop").unwrap())
            .unwrap_err()
            .code,
        "invalidClassificationContext"
    );
    assert!(fake.requested.borrow().is_empty());
    assert_eq!(
        select_image(&fake, parse("https://example.org/image").unwrap())
            .unwrap_err()
            .code,
        "invalidClassificationContext"
    );
}

fn run(fake: &Fake, address: &str, max_chars: usize) -> Result<Selection, ClassificationError> {
    let security = ContentSecurity::new();
    let redaction = Redaction {
        security: &security,
        emails: false,
    };
    select(fake, parse(address)?, &redaction, &|| Ok(()), max_chars)
}

#[test]
fn only_an_https_url_without_credentials_is_accepted() {
    for (address, needle) in [
        ("http://docs.example.org/guide", "HTTPS"),
        ("ftp://docs.example.org/guide", "HTTPS"),
        ("https://user:secret@docs.example.org/guide", "credentials"),
        ("https://token@docs.example.org/guide", "credentials"),
    ] {
        let error = parse(address).unwrap_err();
        assert_eq!(error.code, "securityValidationFailed", "{address}");
        assert!(
            error.message.contains(needle),
            "{address}: {}",
            error.message
        );
        assert!(!error.message.contains("secret"), "{}", error.message);
    }
    assert_eq!(
        parse("not a url").unwrap_err().code,
        "invalidClassificationContext"
    );
    assert!(parse("https://docs.example.org/guide?q=1#rotation").is_ok());
}

#[test]
fn a_fragment_selects_its_heading_section_and_names_it() {
    let fake = Fake::default().page(
        "https://docs.example.org/guide",
        "text/html; charset=utf-8",
        PAGE,
    );
    let selection = run(&fake, "https://docs.example.org/guide#rotation", 10_000).unwrap();
    assert_eq!(
        selection.text,
        "## Rotation\nUpdate the configuration, then restart.\n### Check\nRun status.\n"
    );
    assert_eq!(selection.section.as_deref(), Some("rotation"));
    assert_eq!(
        selection.url.as_str(),
        "https://docs.example.org/guide#rotation"
    );
    assert_eq!(
        *fake.requested.borrow(),
        ["https://docs.example.org/guide"],
        "the fragment is never sent"
    );
}

#[test]
fn without_a_fragment_the_whole_page_is_captured() {
    let fake = Fake::default().page("https://docs.example.org/guide", "text/html", PAGE);
    let selection = run(&fake, "https://docs.example.org/guide", 10_000).unwrap();
    assert_eq!(
        selection.text,
        "# Guide\nIntro.\n## Rotation\nUpdate the configuration, then restart.\n### Check\nRun status.\n## FAQ\nNone.\n"
    );
    assert_eq!(selection.section, None);
}

#[test]
fn a_percent_encoded_fragment_matches_its_decoded_anchor() {
    let page = r#"<h2 id="über uns">Über uns</h2><p>Team.</p>"#;
    let fake = Fake::default().page("https://docs.example.org/a", "text/html", page);
    let selection = run(&fake, "https://docs.example.org/a#%C3%BCber%20uns", 1_000).unwrap();
    assert_eq!(selection.text, "## Über uns\nTeam.\n");
    assert_eq!(selection.section.as_deref(), Some("über uns"));
}

#[test]
fn a_missing_or_ambiguous_anchor_fails_naming_the_fragment() {
    let fake = Fake::default()
        .page("https://docs.example.org/guide", "text/html", PAGE)
        .page(
            "https://docs.example.org/twice",
            "text/html",
            r#"<h2 id="x">A</h2><p>a</p><p id="x">b</p>"#,
        );
    let missing = run(&fake, "https://docs.example.org/guide#absent", 10_000)
        .err()
        .unwrap();
    assert_eq!(missing.code, "classificationContextEmpty");
    assert!(missing.message.contains("#absent"), "{}", missing.message);
    assert!(
        missing.message.contains("No element"),
        "{}",
        missing.message
    );
    let ambiguous = run(&fake, "https://docs.example.org/twice#x", 10_000)
        .err()
        .unwrap();
    assert_eq!(ambiguous.code, "classificationContextEmpty");
    assert!(
        ambiguous.message.contains("2 elements") && ambiguous.message.contains("#x"),
        "{}",
        ambiguous.message
    );
}

#[test]
fn text_over_the_state_limit_fails_instead_of_being_cut() {
    let fake = Fake::default().page("https://docs.example.org/guide", "text/html", PAGE);
    let whole = run(&fake, "https://docs.example.org/guide", 10_000).unwrap();
    let limit = whole.text.chars().count();
    assert!(run(&fake, "https://docs.example.org/guide", limit).is_ok());
    let error = run(&fake, "https://docs.example.org/guide", limit - 1)
        .err()
        .unwrap();
    assert_eq!(error.code, "classificationContextTooLarge");
    assert!(error.message.contains(&format!("{}-character", limit - 1)));
    // The section alone fits where the page does not.
    assert!(run(&fake, "https://docs.example.org/guide#faq", 20).is_ok());
}

#[test]
fn plain_text_and_markdown_are_captured_whole_even_with_a_fragment() {
    let fake = Fake::default()
        .page(
            "https://docs.example.org/notes.txt",
            "text/plain; charset=UTF-8",
            "one\r\ntwo",
        )
        .page(
            "https://docs.example.org/notes.md",
            "text/markdown",
            "# Title\n\n## Part\nbody\n",
        );
    let text = run(&fake, "https://docs.example.org/notes.txt#part", 1_000).unwrap();
    assert_eq!(text.text, "one\ntwo\n");
    assert_eq!(
        text.section, None,
        "a fragment does not apply to plain text"
    );
    let markdown = run(&fake, "https://docs.example.org/notes.md", 1_000).unwrap();
    assert_eq!(markdown.text, "# Title\n\n## Part\nbody\n");
}

#[test]
fn unsupported_content_is_refused_not_judged() {
    let fake = Fake::default()
        .bytes(
            "https://e.example.org/doc.pdf",
            Some("application/pdf"),
            b"%PDF-1.7".to_vec(),
        )
        .bytes("https://e.example.org/untyped", None, b"text".to_vec())
        .bytes(
            "https://e.example.org/latin1",
            Some("text/html; charset=ISO-8859-1"),
            b"<p>caf\xe9</p>".to_vec(),
        )
        .bytes(
            "https://e.example.org/invalid",
            Some("text/plain"),
            b"caf\xe9 ok".to_vec(),
        )
        .bytes(
            "https://e.example.org/binary",
            Some("text/plain"),
            b"a\0b\0c\0".to_vec(),
        )
        .page(
            "https://e.example.org/app",
            "text/html",
            r#"<div id="root"></div><script src="/app.js"></script>"#,
        );
    for (path, needle) in [
        ("doc.pdf", "application/pdf"),
        ("untyped", "untyped"),
        ("latin1", "iso-8859-1"),
        ("invalid", "UTF-8"),
        ("binary", "binary"),
        ("app", "scripts"),
    ] {
        let error = run(&fake, &format!("https://e.example.org/{path}"), 1_000)
            .err()
            .unwrap();
        assert_eq!(error.code, "classificationContextUnavailable", "{path}");
        assert!(error.message.contains(needle), "{path}: {}", error.message);
    }
    let empty = Fake::default().page("https://e.example.org/blank", "text/html", "<p> </p>");
    assert_eq!(
        run(&empty, "https://e.example.org/blank", 1_000)
            .err()
            .unwrap()
            .code,
        "classificationContextEmpty"
    );
}

#[test]
fn a_byte_order_mark_is_not_page_text() {
    let fake = Fake::default().page("https://e.example.org/t", "text/plain", "\u{feff}hello");
    assert_eq!(
        run(&fake, "https://e.example.org/t", 100).unwrap().text,
        "hello\n"
    );
}

#[test]
fn redirects_resolve_relative_locations_and_keep_the_fragment() {
    let fake = Fake::default()
        .status("https://a.example.org/old", 301, Some("/moved"))
        .status(
            "https://a.example.org/moved",
            308,
            Some("https://b.example.org/guide"),
        )
        .page("https://b.example.org/guide", "text/html", PAGE);
    let selection = run(&fake, "https://a.example.org/old#faq", 1_000).unwrap();
    assert_eq!(selection.url.as_str(), "https://b.example.org/guide#faq");
    assert_eq!(selection.text, "## FAQ\nNone.\n");
    assert_eq!(
        *fake.requested.borrow(),
        [
            "https://a.example.org/old",
            "https://a.example.org/moved",
            "https://b.example.org/guide"
        ]
    );
}

#[test]
fn a_redirect_is_rechecked_as_https_without_credentials() {
    let fake = Fake::default()
        .status(
            "https://a.example.org/down",
            302,
            Some("http://a.example.org/plain"),
        )
        .status(
            "https://a.example.org/creds",
            302,
            Some("https://u:p@a.example.org/x"),
        );
    for path in ["down", "creds"] {
        let error = run(&fake, &format!("https://a.example.org/{path}"), 1_000)
            .err()
            .unwrap();
        assert_eq!(error.code, "securityValidationFailed", "{path}");
    }
    assert_eq!(
        fake.requested.borrow().len(),
        2,
        "the refused hop is never requested"
    );
}

#[test]
fn at_most_five_redirects_are_followed() {
    let mut fake = Fake::default();
    let hops: Vec<String> = (0..=6)
        .map(|n| format!("https://r.example.org/{n}"))
        .collect();
    let locations: Vec<&'static str> = (1..=6)
        .map(|n| &*Box::leak(format!("/{n}").into_boxed_str()))
        .collect();
    for n in 0..6 {
        fake = fake.status(&hops[n], 302, Some(locations[n]));
    }
    fake = fake.page(&hops[5], "text/plain", "five");
    assert_eq!(
        run(&fake, &hops[0], 100).unwrap().text,
        "five\n",
        "five redirects are followed"
    );
    let mut fake = Fake::default();
    for n in 0..6 {
        fake = fake.status(&hops[n], 302, Some(locations[n]));
    }
    fake = fake.page(&hops[6], "text/plain", "six");
    let error = run(&fake, &hops[0], 100).err().unwrap();
    assert_eq!(error.code, "classificationContextFailed");
    assert!(error.message.contains("more than 5"), "{}", error.message);
    assert_eq!(fake.requested.borrow().len(), 6);
}

#[test]
fn http_failures_and_redirects_without_location_fail_the_capture() {
    let fake = Fake::default()
        .status("https://a.example.org/private", 403, None)
        .status("https://a.example.org/nowhere", 302, None);
    let forbidden = run(&fake, "https://a.example.org/private", 100)
        .err()
        .unwrap();
    assert_eq!(forbidden.code, "classificationContextFailed");
    assert!(
        forbidden.message.contains("HTTP 403"),
        "{}",
        forbidden.message
    );
    let nowhere = run(&fake, "https://a.example.org/nowhere", 100)
        .err()
        .unwrap();
    assert_eq!(nowhere.code, "classificationContextFailed");
    assert!(nowhere.message.contains("Location"), "{}", nowhere.message);
}

#[test]
fn captured_text_is_redacted_like_every_reader_output() {
    let secret = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    let page = format!("<p>token {secret} here</p>");
    let fake = Fake::default().bytes(
        "https://a.example.org/t",
        Some("text/html"),
        page.into_bytes(),
    );
    let text = run(&fake, "https://a.example.org/t", 1_000).unwrap().text;
    assert!(!text.contains(secret), "{text}");
    assert!(
        text.starts_with("token ") && text.ends_with(" here\n"),
        "{text}"
    );
}

/// A temporary Octocode home and workspace whose path policy admits the
/// capture directory.
struct Home {
    _root: tempfile::TempDir,
    home: PathBuf,
    workspace: PathBuf,
    policy: PathPolicy,
}

impl Home {
    fn new() -> Self {
        let root = tempfile::TempDir::new().unwrap();
        let base = root.path().canonicalize().unwrap();
        let home = base.join("home");
        let workspace = base.join("work");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(workspace.clone()),
            additional_roots: vec![capture_root(&home)],
            include_home: false,
            home_dir: Some(home.clone()),
        })
        .unwrap();
        Self {
            _root: root,
            home,
            workspace,
            policy,
        }
    }

    fn store(&self) -> Store<'_> {
        Store::at(
            capture_root(&self.home),
            Duration::from_secs(3_600),
            &self.policy,
        )
        .unwrap()
    }

    /// A runtime with this home and the capture directory allowed.
    fn runtime(&self) -> ToolRuntime {
        let path = |path: &Path| path.to_string_lossy().into_owned();
        let env = BTreeMap::from([
            ("OCTOCODE_ENABLE_LOCAL".to_owned(), "true".to_owned()),
            ("OCTOCODE_HOME".to_owned(), path(&self.home)),
            ("WORKSPACE_ROOT".to_owned(), path(&self.workspace)),
            (
                "ALLOWED_PATHS".to_owned(),
                format!(
                    "{},{}",
                    path(&self.workspace),
                    path(&capture_root(&self.home))
                ),
            ),
            ("OCTOCODE_ENABLE_STATS".to_owned(), "false".to_owned()),
        ]);
        ToolRuntime::new(ConfigInput {
            env,
            cwd: self.workspace.clone(),
            os_home: self.home.clone(),
            global_env: FileInput::Missing {
                path: self.home.join(".env"),
            },
            project_env: FileInput::Missing {
                path: self.workspace.join(".octocode/.env"),
            },
            config_file: FileInput::Missing {
                path: self.home.join(".octocoderc"),
            },
            project_config_file: FileInput::Missing {
                path: self.workspace.join(".octocode/.octocoderc"),
            },
            runtime_surface: RuntimeSurface::Cli,
        })
        .unwrap()
    }
}

fn captured_page(home: &Home) -> (Selection, Captured) {
    let fake = Fake::default().page("https://docs.example.org/guide", "text/html", PAGE);
    let selection = run(&fake, "https://docs.example.org/guide#rotation", 10_000).unwrap();
    let copy = Selection {
        url: selection.url.clone(),
        section: selection.section.clone(),
        text: selection.text.clone(),
    };
    (copy, home.store().save(selection).unwrap())
}

#[test]
fn a_capture_is_saved_immutably_and_its_lead_reads_exactly_those_bytes() {
    let home = Home::new();
    let (selection, captured) = captured_page(&home);
    let snapshot = octocode_engine::digest::sha256(selection.text.as_bytes());
    let file = capture_root(&home.home).join(&snapshot).join("source.txt");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), selection.text);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "owner-only capture file");
    }
    assert_eq!(
        captured.state,
        json!({
            "source": {"url": "https://docs.example.org/guide#rotation", "section": "rotation"},
            "text": selection.text,
        })
    );
    let source = captured.source.as_ref().unwrap();
    assert_eq!(source["url"], "https://docs.example.org/guide#rotation");
    assert_eq!(source["snapshot"], snapshot.as_str());
    assert_eq!(captured.read["tool"], "localFetch");
    let row = &captured.read["query"]["queries"][0];
    assert_eq!(
        *row,
        json!({
            "path": file,
            "snapshot": snapshot,
            "ranges": [format!("1-{}", selection.text.lines().count())],
        })
    );
    assert_eq!(row["ranges"], json!(["1-4"]));
}

#[test]
fn expiry_follows_the_existing_retention_ttl() {
    let home = Home::new();
    let before = crate::civil_date::unix_millis();
    let (_, captured) = captured_page(&home);
    let after = crate::civil_date::unix_millis();
    let expires = captured.source.unwrap()["expiresAt"]
        .as_str()
        .unwrap()
        .to_owned();
    let earliest = crate::civil_date::iso8601_millis(before + 3_600_000);
    let latest = crate::civil_date::iso8601_millis(after + 3_600_000);
    assert!(
        earliest <= expires && expires <= latest,
        "{earliest} <= {expires} <= {latest}"
    );
    assert!(expires.ends_with('Z'), "{expires}");
}

#[test]
fn the_same_text_reuses_its_content_address() {
    let home = Home::new();
    let (_, first) = captured_page(&home);
    let (_, second) = captured_page(&home);
    assert_eq!(first.read, second.read);
    let entries = std::fs::read_dir(capture_root(&home.home)).unwrap().count();
    assert_eq!(entries, 1);
}

#[test]
fn a_capture_directory_outside_the_path_policy_is_refused_before_saving() {
    let home = Home::new();
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(home.workspace.clone()),
        additional_roots: Vec::new(),
        include_home: false,
        home_dir: Some(home.home.clone()),
    })
    .unwrap();
    let error = Store::at(capture_root(&home.home), Duration::from_secs(60), &policy)
        .err()
        .unwrap();
    assert_eq!(error.code, "classificationContextUnavailable");
    assert!(error.message.contains("path policy"), "{}", error.message);
    assert_eq!(
        std::fs::read_dir(capture_root(&home.home)).unwrap().count(),
        0,
        "nothing is saved"
    );
}

async fn local_fetch(runtime: &ToolRuntime, query: Value) -> Value {
    let admission = runtime.admit("url-capture-lead".into()).unwrap();
    runtime
        .execute(admission, "localFetch".into(), query, Channel::Structured)
        .await
        .unwrap()
        .structured_content["results"][0]["data"]
        .clone()
}

/// clasify sanitizes its whole row envelope before returning it. The lead's
/// absolute capture path and snapshot, and the provenance, must come through
/// unchanged, so the copied lead still reads the captured text.
#[tokio::test(flavor = "multi_thread")]
async fn the_lead_survives_output_sanitization_and_still_reads_the_capture() {
    let home = Home::new();
    let (selection, captured) = captured_page(&home);
    let snapshot = octocode_engine::digest::sha256(selection.text.as_bytes());
    let installed = json!({
        "tool": "localFetch",
        "query": {"queries": [{
            "path": format!("/Users/someone/.octocode/tmp/capture/url/{snapshot}/source.txt"),
            "snapshot": snapshot,
            "ranges": ["1-3"],
        }]},
    });
    let row =
        |read: &Value| json!({"queries": [{"source": captured.source, "hints": {"read": read}}]});
    let context = crate::execution::ExecutionContext {
        cancellation: tokio_util::sync::CancellationToken::new(),
        deadline: std::time::Instant::now() + Duration::from_secs(5),
        walk_threads: None,
        response_window: None,
        github_credential: None,
    };
    for read in [&captured.read, &installed] {
        let mut sanitized = row(read);
        crate::response::rows::sanitize_fields(&mut sanitized, &ContentSecurity::new(), &context)
            .unwrap();
        assert_eq!(sanitized, row(read));
    }
    let runtime = home.runtime();
    let mut sanitized = row(&captured.read);
    crate::response::rows::sanitize_fields(&mut sanitized, &ContentSecurity::new(), &context)
        .unwrap();
    let data = local_fetch(
        &runtime,
        sanitized["queries"][0]["hints"]["read"]["query"].clone(),
    )
    .await;
    assert!(data.get("errorCode").is_none(), "{data}");
    assert_eq!(data["totalLines"], selection.text.lines().count(), "{data}");
    runtime.close().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_read_lead_returns_the_captured_text_through_local_fetch() {
    let home = Home::new();
    let (selection, captured) = captured_page(&home);
    let runtime = home.runtime();
    let data = local_fetch(&runtime, captured.read["query"].clone()).await;
    assert!(data.get("errorCode").is_none(), "{data}");
    // localFetch numbers each line `N<TAB>`; the lines are the captured ones.
    let numbered: String = selection
        .text
        .lines()
        .enumerate()
        .map(|(index, line)| format!("{}\t{line}\n", index + 1))
        .collect();
    assert_eq!(data["content"], numbered, "{data}");
    assert_eq!(data["totalLines"], selection.text.lines().count(), "{data}");
    // Tampered bytes no longer match the capture's snapshot.
    let file = captured.read["query"]["queries"][0]["path"]
        .as_str()
        .unwrap()
        .to_owned();
    std::fs::write(&file, "changed\n").unwrap();
    let stale = local_fetch(&runtime, captured.read["query"].clone()).await;
    assert_eq!(stale["errorCode"], "staleSnapshot", "{stale}");
    runtime.close().await;
}

#[test]
fn a_stopped_call_ends_html_extraction_with_its_own_reason() {
    let fake = Fake::default().page("https://docs.example.org/guide", "text/html", PAGE);
    let security = ContentSecurity::new();
    let redaction = Redaction {
        security: &security,
        emails: false,
    };
    for reason in [
        crate::execution::ExecutionError::Timeout,
        crate::execution::ExecutionError::Cancelled,
    ] {
        let expected = if reason == crate::execution::ExecutionError::Timeout {
            "timeout"
        } else {
            "cancelled"
        };
        let check = || Err(stopped(reason));
        let error = select(
            &fake,
            parse("https://docs.example.org/guide").unwrap(),
            &redaction,
            &check,
            10_000,
        )
        .err()
        .unwrap();
        assert_eq!(error.code, expected);
    }
}
