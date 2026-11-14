//! The `lspSearch` lead other tools offer for a declaration they found.
use octocode_contract_types::TestCode;
use octocode_engine::lsp::config::{
    LspDiscoveryOptions, default_server_for_file, server_command_resolves,
};
use serde_json::{Value, json};
use std::path::Path;

/// What a lead asks about the declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verify {
    /// Its call sites, each with the enclosing caller.
    Callers,
    /// Every reference, calls or not.
    References,
}

impl Verify {
    /// `Callers` for a callable declaration kind, `References` otherwise.
    #[must_use]
    pub fn for_kind(kind: &str) -> Self {
        match kind {
            "function" | "method" | "fn" | "constructor" | "def" | "func" => Self::Callers,
            _ => Self::References,
        }
    }

    fn operation(self) -> &'static str {
        match self {
            Self::Callers => "callers",
            Self::References => "references",
        }
    }
}

/// The lead name of a [`verify_query`] row: `findCallers` or
/// `findReferences`.
pub(crate) fn lead_name(row: &Value) -> String {
    match row.get("operation").and_then(Value::as_str) {
        Some("callers") => "findCallers",
        _ => "findReferences",
    }
    .to_owned()
}

/// The `lspSearch` query row for the declaration `symbol` at one-based
/// `line` of `path`, with the caller's `test_code` (both operations a lead
/// runs, `callers` and `references`, take it). `None` when no level above
/// `syntax` would answer for the file ([`level_above_syntax`]): a `syntax`
/// answer is the tree-sitter facts the producer already shows, so a lead
/// that could only repeat them, or fail, is never offered.
#[must_use]
pub fn verify_query(
    path: &str,
    symbol: &str,
    line: u64,
    verify: Verify,
    test_code: Option<TestCode>,
) -> Option<Value> {
    level_above_syntax(path).then(|| {
        let mut row = json!({
            "path": path,
            "operation": verify.operation(),
            "symbolName": symbol,
            "line": line,
        });
        if let Some(test_code) = test_code {
            row["testCode"] = json!(test_code);
        }
        row
    })
}

/// Whether the staged ladder would answer for `path` above `syntax`: an
/// editor that has the file's folder open (the IDE rung,
/// [`crate::tools::editor_lock::editor_covers`]), or a discovered server that
/// would launch ([`server_available`]).
fn level_above_syntax(path: &str) -> bool {
    let discovery = discovery().for_one_request();
    let workspace = workspace_of(path, &discovery);
    server_available(path, &workspace, &discovery) || {
        // The rung takes canonical paths; the lead keeps the producer's.
        let canonical = std::fs::canonicalize(path).map_or_else(
            |_| path.to_owned(),
            |real| real.to_string_lossy().into_owned(),
        );
        crate::tools::editor_lock::editor_covers(&discovery, &canonical, &workspace)
    }
}

/// The workspace a request for `path` resolves: its project root, else the
/// file's directory.
fn workspace_of(path: &str, discovery: &LspDiscoveryOptions) -> String {
    octocode_engine::lsp::config::workspace_root_for_file(path, discovery).unwrap_or_else(|_| {
        Path::new(path)
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".into())
    })
}

/// Whether the server discovered for this lead would launch: never by running
/// it. A lead hangs on a search hit, so the check costs file stats, and a rustup
/// proxy's one question is kept while what rustup reads is unchanged
/// ([`server_command_resolves`]). Installation and removal are observed
/// immediately.
fn server_available(path: &str, workspace: &str, discovery: &LspDiscoveryOptions) -> bool {
    default_server_for_file(path, workspace, discovery)
        .is_some_and(|config| server_command_resolves(&config))
}

thread_local! {
    /// Discovery the runtime resolved for the request running on this
    /// thread (see [`with_lead_discovery`]).
    static LEAD_DISCOVERY: std::cell::RefCell<Option<LspDiscoveryOptions>> =
        const { std::cell::RefCell::new(None) };
}

/// Run `run` (one local tool row, on the calling thread) with the runtime's
/// resolved LSP settings, `options`, as the discovery every lead in it probes.
pub fn with_lead_discovery<R>(options: LspDiscoveryOptions, run: impl FnOnce() -> R) -> R {
    struct Restore(Option<LspDiscoveryOptions>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            LEAD_DISCOVERY.with(|cell| *cell.borrow_mut() = previous);
        }
    }
    let _restore = Restore(LEAD_DISCOVERY.with(|cell| cell.borrow_mut().replace(options)));
    run()
}

/// The runtime-resolved discovery of the current request. Every dispatched
/// local row runs inside [`with_lead_discovery`]; a caller outside one (a unit
/// test) probes the built-in defaults, never the bare process environment.
fn discovery() -> LspDiscoveryOptions {
    LEAD_DISCOVERY
        .with(|cell| cell.borrow().clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// [`server_available`] as a lead probes it.
    fn server_resolves(path: &str) -> bool {
        let discovery = discovery();
        server_available(path, &workspace_of(path, &discovery), &discovery)
    }

    #[cfg(unix)]
    #[test]
    fn each_lead_observes_server_installation_and_removal_immediately() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("directory");
        let command = dir.path().join("fixture-lsp");
        let config = dir.path().join("lsp-servers.json");
        std::fs::write(&config, serde_json::json!({"languageServers":{".fixture":{"command":command,"args":[],"languageId":"fixture"}}}).to_string()).expect("config");
        let file = dir.path().join("source.fixture");
        std::fs::write(&file, "target").expect("source");
        let options = LspDiscoveryOptions {
            octocode_home: Some(dir.path().to_path_buf()),
            ..Default::default()
        };
        with_lead_discovery(options, || {
            assert!(!server_resolves(file.to_str().expect("path")));
            std::fs::write(&command, "#!/bin/sh\nexit 0\n").expect("server");
            std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o755))
                .expect("executable");
            assert!(
                server_resolves(file.to_str().expect("path")),
                "newly installed server is immediately discoverable"
            );
            std::fs::remove_file(&command).expect("remove server");
            assert!(
                !server_resolves(file.to_str().expect("path")),
                "removed server no longer produces a lead"
            );
        });
    }

    #[test]
    fn leads_probe_the_runtime_resolved_discovery_inside_a_row_only() {
        let options = LspDiscoveryOptions {
            env: BTreeMap::from([("OCTOCODE_LEAD_PROBE".to_owned(), "resolved".to_owned())]),
            ..LspDiscoveryOptions::default()
        };
        let inside = with_lead_discovery(options, discovery);
        assert_eq!(
            inside.env.get("OCTOCODE_LEAD_PROBE").map(String::as_str),
            Some("resolved")
        );
        let after = discovery();
        assert!(after.env.is_empty(), "{after:?}");
        assert!(after.octocode_home.is_none());
    }

    /// A lead hangs on a search hit: deciding it runs no server, whatever the
    /// server is. The script logs every execution; rust-analyzer was once asked
    /// `--version` for each hit.
    #[cfg(unix)]
    #[test]
    fn a_lead_never_runs_the_server() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("directory");
        let ran = dir.path().join("ran");
        let command = dir.path().join("rust-analyzer");
        std::fs::write(
            &command,
            format!("#!/bin/sh\necho \"$*\" >> '{}'\nexit 0\n", ran.display()),
        )
        .expect("server");
        std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
        let config = dir.path().join("lsp-servers.json");
        std::fs::write(&config, serde_json::json!({"languageServers":{".rs":{"command":command,"args":[],"languageId":"rust"}}}).to_string()).expect("config");
        let file = dir.path().join("a.rs");
        std::fs::write(&file, "fn a() {}\n").expect("source");
        let options = LspDiscoveryOptions {
            octocode_home: Some(dir.path().to_path_buf()),
            ..Default::default()
        };
        with_lead_discovery(options, || {
            for _ in 0..3 {
                assert!(server_resolves(file.to_str().expect("path")));
            }
            std::fs::remove_file(&command).expect("removed");
            assert!(!server_resolves(file.to_str().expect("path")));
        });
        assert!(!ran.exists(), "{:?}", std::fs::read_to_string(&ran));
    }

    /// A lead that could only be refused is not offered: fetched code answers
    /// `lspSearch` with a policy refusal.
    #[cfg(unix)]
    #[test]
    fn no_lead_points_into_fetched_code() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("fixture");
        let command = dir.path().join("fixture-lsp");
        std::fs::write(&command, "#!/bin/sh\nexit 0\n").expect("server");
        std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
        std::fs::write(dir.path().join("lsp-servers.json"), serde_json::json!({"languageServers":{".fixture":{"command":command,"args":[],"languageId":"fixture"}}}).to_string()).expect("config");
        let clones = dir.path().join("clones");
        std::fs::create_dir(&clones).expect("clones");
        let file = clones.join("a.fixture");
        std::fs::write(&file, "function a() {}\n").expect("file");
        let path = file.to_string_lossy().into_owned();
        let options = |fetched_code_roots| LspDiscoveryOptions {
            octocode_home: Some(dir.path().to_path_buf()),
            fetched_code_roots,
            ..Default::default()
        };
        with_lead_discovery(options(vec![clones.clone()]), || {
            assert_eq!(verify_query(&path, "a", 1, Verify::Callers, None), None);
        });
        with_lead_discovery(options(Vec::new()), || {
            assert!(verify_query(&path, "a", 1, Verify::Callers, None).is_some());
        });
    }

    /// A declaration lead runs `callers` for a callable and `references`
    /// otherwise, and is withheld when no language server would start for the
    /// file.
    #[cfg(unix)]
    #[test]
    fn verify_query_picks_the_operation_and_needs_a_server() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("fixture");
        // A fixture server for `.fixture`, so the test never depends on the
        // language servers installed on this machine.
        let command = dir.path().join("fixture-lsp");
        std::fs::write(&command, "#!/bin/sh\nexit 0\n").expect("server");
        std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
        let config = dir.path().join("lsp-servers.json");
        std::fs::write(&config, serde_json::json!({"languageServers":{".fixture":{"command":command,"args":[],"languageId":"fixture"}}}).to_string()).expect("config");
        let options = LspDiscoveryOptions {
            octocode_home: Some(dir.path().to_path_buf()),
            ..Default::default()
        };
        let file = dir.path().join("a.fixture");
        std::fs::write(&file, "function a() {}\n").expect("file");
        let path = file.to_string_lossy().into_owned();
        let unknown = dir.path().join("a.unknownext");
        std::fs::write(&unknown, "a\n").expect("file");
        with_lead_discovery(options, || {
            let callers = verify_query(&path, "a", 1, Verify::for_kind("function"), None)
                .expect("fixture server");
            assert_eq!(
                callers,
                serde_json::json!({"path": path, "operation": "callers", "symbolName": "a", "line": 1})
            );
            let references = verify_query(&path, "A", 3, Verify::for_kind("class"), None)
                .expect("fixture server");
            assert_eq!(references["operation"], "references");
            assert_eq!(
                verify_query(&unknown.to_string_lossy(), "a", 1, Verify::Callers, None),
                None,
                "no server for the extension: no lead"
            );
        });
    }

    /// A fixture server for `.fixture` under a fresh home: tests never
    /// depend on the language servers installed on this machine.
    #[cfg(unix)]
    fn fixture_server(home: &Path) -> LspDiscoveryOptions {
        use std::os::unix::fs::PermissionsExt;
        let command = home.join("fixture-lsp");
        std::fs::write(&command, "#!/bin/sh\nexit 0\n").expect("server");
        std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
        std::fs::write(home.join("lsp-servers.json"), serde_json::json!({"languageServers":{".fixture":{"command":command,"args":[],"languageId":"fixture"}}}).to_string()).expect("config");
        LspDiscoveryOptions {
            octocode_home: Some(home.to_path_buf()),
            ..Default::default()
        }
    }

    /// The lead runs with the caller's `testCode`: a search that left tests
    /// out asks for callers and references outside tests too. A caller that
    /// set none sends none, so lspSearch's own default applies.
    #[cfg(unix)]
    #[test]
    fn the_lead_keeps_the_callers_test_code() {
        let dir = tempfile::tempdir().expect("fixture");
        let options = fixture_server(dir.path());
        let file = dir.path().join("a.fixture");
        std::fs::write(&file, "function a() {}\n").expect("file");
        let path = file.to_string_lossy().into_owned();
        with_lead_discovery(options, || {
            for verify in [Verify::Callers, Verify::References] {
                let row = verify_query(&path, "a", 1, verify, Some(TestCode::Exclude))
                    .expect("fixture server");
                assert_eq!(row["testCode"], "exclude", "{row}");
                assert_eq!(row["operation"], verify.operation(), "{row}");
                let only = verify_query(&path, "a", 1, verify, Some(TestCode::Only))
                    .expect("fixture server");
                assert_eq!(only["testCode"], "only", "{only}");
                let unset = verify_query(&path, "a", 1, verify, None).expect("fixture server");
                assert!(unset.get("testCode").is_none(), "{unset}");
            }
        });
    }

    /// A lock file of a live editor (this process) with `folder` open.
    #[cfg(unix)]
    fn write_editor_lock(home: &Path, folder: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let dir = home.join("ide");
        std::fs::create_dir_all(&dir).expect("lock directory");
        let lock = dir.join(format!("{}.json", std::process::id()));
        std::fs::write(
            &lock,
            serde_json::json!({
                "version": 1, "pid": std::process::id(), "ide": "Visual Studio Code",
                "port": 1, "token": "0123456789abcdef", "workspaceFolders": [folder]
            })
            .to_string(),
        )
        .expect("lock");
        std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o600)).expect("mode");
    }

    /// No server would start for the file, but an editor that has its folder
    /// open answers lspSearch at the IDE rung: the lead is offered. An editor
    /// of another folder, or one over fetched code, offers none.
    #[cfg(unix)]
    #[test]
    fn an_editor_lock_that_covers_the_file_offers_the_lead_without_a_server() {
        let dir = tempfile::tempdir().expect("fixture");
        let home = dir.path().join("home");
        let project = dir.path().join("project");
        let other = dir.path().join("other");
        for folder in [&home, &project, &other] {
            std::fs::create_dir_all(folder).expect("folder");
        }
        let file = project.join("a.fixture");
        std::fs::write(&file, "function a() {}\n").expect("file");
        // The temporary directory's own spelling, not resolved: a lead's
        // path is what the producer printed.
        let path = file.to_string_lossy().into_owned();
        let options = |fetched_code_roots| LspDiscoveryOptions {
            octocode_home: Some(home.clone()),
            fetched_code_roots,
            ..Default::default()
        };
        with_lead_discovery(options(Vec::new()), || {
            assert_eq!(
                verify_query(&path, "a", 1, Verify::Callers, None),
                None,
                "no server and no editor"
            );
            write_editor_lock(&home, &other);
            assert_eq!(
                verify_query(&path, "a", 1, Verify::Callers, None),
                None,
                "the editor has another folder open"
            );
            write_editor_lock(&home, &project);
            assert_eq!(
                verify_query(&path, "a", 1, Verify::Callers, Some(TestCode::Exclude)),
                Some(serde_json::json!({
                    "path": path, "operation": "callers", "symbolName": "a", "line": 1,
                    "testCode": "exclude"
                }))
            );
        });
        with_lead_discovery(options(vec![project.clone()]), || {
            assert_eq!(
                verify_query(&path, "a", 1, Verify::Callers, None),
                None,
                "fetched code: the IDE rung does not apply"
            );
        });
    }
}
