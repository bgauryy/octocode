use super::*;
use crate::config::{ConfigInput, FileInput, RuntimeSurface};
use crate::policy::path::{PathPolicy, PathPolicyConfig};
use crate::runtime::{Channel, ToolRuntime};
use std::collections::BTreeMap;
use std::time::Duration;

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
    assert_eq!(error.code, ErrorCode::ClassificationContextUnavailable);
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
        .execute(admission, "localFetch".into(), query, Channel::Cli)
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
        let sanitized = ContentSecurity::new()
            .seal_output(row(read), false, || context.check())
            .unwrap();
        assert_eq!(sanitized, row(read));
    }
    let runtime = home.runtime();
    let sanitized = ContentSecurity::new()
        .seal_output(row(&captured.read), false, || context.check())
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
