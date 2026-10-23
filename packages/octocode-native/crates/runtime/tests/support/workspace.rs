//! The isolated fixture workspace: temp root, home, token and the runtime built over them.

use super::{FIXTURE_LIMITS, UNMOCKED_GITHUB_API_URL};
use octocode_native::config::{ConfigInput, FileInput, RuntimeSurface};
use octocode_native::execution::RuntimeLimits;
use octocode_native::runtime::ToolRuntime;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Per-fixture token suffix. The GitHub rate-limit budget is process-wide and
/// keyed by API host + token digest; wiremock pools its servers, so a later
/// test can reuse an earlier test's `127.0.0.1:<port>`. A distinct token per
/// workspace keeps one test's simulated limits, cooldowns, pacing, and circuit
/// state from leaking into another.
static FIXTURE_SEQ: AtomicU64 = AtomicU64::new(0);

pub struct Workspace {
    _root: tempfile::TempDir,
    pub workspace: PathBuf,
    pub home: PathBuf,
    pub token: String,
}

impl Workspace {
    pub fn new() -> Self {
        let root = tempfile::TempDir::new().expect("temporary workspace");
        let root_path = root
            .path()
            .canonicalize()
            .expect("canonical temporary root");
        let workspace = root_path.join("work");
        let home = root_path.join("home");
        std::fs::create_dir_all(&workspace).expect("workspace directory");
        std::fs::create_dir_all(&home).expect("home directory");
        let token = format!(
            "fixture-token-{}",
            FIXTURE_SEQ.fetch_add(1, Ordering::Relaxed)
        );
        Self {
            _root: root,
            workspace,
            home,
            token,
        }
    }

    pub fn write(&self, relative: &str, contents: impl AsRef<[u8]>) -> PathBuf {
        let path = self.workspace.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("fixture parents");
        }
        std::fs::write(&path, contents).expect("fixture write");
        path
    }

    /// `PATH` whose `gh` answers only `gh auth token --hostname 127.0.0.1`
    /// (with no token env set) and logs each call to `$OCTOCODE_HOME/gh-calls`.
    #[cfg(unix)]
    pub fn fake_gh_path(&self) -> String {
        use std::os::unix::fs::PermissionsExt;
        let script = self.write("bin/gh", "#!/bin/sh\n[ \"$1 $2 $3 $4\" = 'auth token --hostname 127.0.0.1' ] || exit 2\n[ -z \"$GH_TOKEN$GITHUB_TOKEN\" ] || exit 3\nprintf x >> \"$OCTOCODE_HOME/gh-calls\"\nprintf synthetic-gh-credential\n");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
            .expect("fake gh permissions");
        format!(
            "{}:/usr/bin:/bin",
            script.parent().expect("fake gh dir").display()
        )
    }

    pub fn write_outside_allowed_roots(
        &self,
        relative: &str,
        contents: impl AsRef<[u8]>,
    ) -> PathBuf {
        let path = self._root.path().join("outside").join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("outside fixture parents");
        }
        std::fs::write(&path, contents).expect("outside fixture write");
        path
    }

    pub fn config(&self, extra: &[(&str, String)]) -> ConfigInput {
        let mut env = BTreeMap::from([
            ("OCTOCODE_ENABLE_LOCAL".into(), "true".into()),
            (
                "ALLOWED_PATHS".into(),
                self.workspace.to_string_lossy().into_owned(),
            ),
            (
                "WORKSPACE_ROOT".into(),
                self.workspace.to_string_lossy().into_owned(),
            ),
            (
                "OCTOCODE_HOME".into(),
                self.home.to_string_lossy().into_owned(),
            ),
            // The OS home (`os_home` below) as child processes see it: without
            // it, a spawned `gh` or `git` resolves an empty home and writes its
            // state relative to the test's working directory.
            ("HOME".into(), self.home.to_string_lossy().into_owned()),
            ("GITHUB_TOKEN".into(), self.token.clone()),
            ("GITHUB_API_URL".into(), UNMOCKED_GITHUB_API_URL.into()),
            ("OCTOCODE_ENABLE_STATS".into(), "false".into()),
            // Fixture workspaces trust their own language-server definitions.
            (
                octocode_native::config::field::LSP_TRUST_PROJECT_CONFIG
                    .env_name()
                    .expect("lsp.trustProjectConfig is bound")
                    .into(),
                "true".into(),
            ),
        ]);
        for (key, value) in extra {
            env.insert((*key).into(), value.clone());
        }
        ConfigInput {
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
        }
    }

    pub fn runtime(&self, extra: &[(&str, String)]) -> ToolRuntime {
        self.runtime_with(extra, FIXTURE_LIMITS)
    }

    /// [`Self::runtime`] under the internal `limits` (the typed test seam;
    /// limits are code constants, never configuration).
    pub fn runtime_with(&self, extra: &[(&str, String)], limits: RuntimeLimits) -> ToolRuntime {
        ToolRuntime::with_limits(self.config(extra), limits).expect("native runtime")
    }
}
