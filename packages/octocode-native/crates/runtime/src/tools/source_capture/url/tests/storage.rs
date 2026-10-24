use super::*;
use crate::policy::path::{PathPolicy, PathPolicyConfig};
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
    let row = &captured.read["input"]["queries"][0];
    assert_eq!(
        *row,
        json!({
            "path": file,
            "snapshot": snapshot,
            "ranges": [{"startLine":1,"endLine":selection.text.lines().count()}],
        })
    );
    assert_eq!(row["ranges"], json!([{"startLine":1,"endLine":4}]));
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

// Executable captured reads are exercised through the real cursor registry in
// runtime_clasify/sources.rs; storage tests do not mint or decode public handles.
#[test]
fn capture_identity_and_scoped_read_survive_output_sanitization() {
    let home = Home::new();
    let (_, captured) = captured_page(&home);
    let envelope = json!({"results":[{"index":0,"data":{"source":captured.source,"next":{"read":captured.read}}}]});
    let sanitized = ContentSecurity::new()
        .seal_output(envelope.clone(), false, || Ok::<_, String>(()))
        .unwrap();
    assert_eq!(sanitized, envelope);
    let read = &sanitized["results"][0]["data"]["next"]["read"];
    assert_eq!(read["kind"], "continue");
    crate::contracts::validate_built_query("localFetch", read["input"]["queries"][0].clone())
        .expect("valid retained source identity");
}
