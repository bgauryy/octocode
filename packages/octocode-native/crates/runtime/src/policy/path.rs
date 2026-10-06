use std::path::{Component, Path, PathBuf};

use super::discovery::{is_sensitive_path, withheld_message};
use super::{PolicyError, PolicyErrorCode};

#[derive(Clone, Debug, Default)]
pub struct PathPolicyConfig {
    pub workspace_root: Option<PathBuf>,
    pub additional_roots: Vec<PathBuf>,
    pub include_home: bool,
    pub home_dir: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedPath {
    pub canonical: PathBuf,
    pub display: String,
}

#[derive(Clone, Debug)]
pub struct PathPolicy {
    roots: Vec<PathBuf>,
    workspace_root: Option<PathBuf>,
    /// The workspace root with symlinks resolved (e.g. macOS `/var` →
    /// `/private/var`), so canonical paths still display workspace-relative.
    workspace_real: Option<PathBuf>,
    home_dir: Option<PathBuf>,
}

/// A search walk enters a directory the policy admits and reads a file it
/// admits for reading.
impl octocode_engine::portable::RipgrepPathFilter for PathPolicy {
    fn allows(&self, path: &Path, is_dir: bool) -> bool {
        if is_dir {
            self.validate(path).is_ok()
        } else {
            self.validate_read(path).is_ok()
        }
    }
}

impl PathPolicy {
    pub fn new(config: PathPolicyConfig) -> Result<Self, PolicyError> {
        let home = config.home_dir.or_else(default_home);
        let mut roots = Vec::new();
        for root in config
            .workspace_root
            .iter()
            .chain(config.additional_roots.iter())
            .chain(config.include_home.then_some(home.as_ref()).flatten())
        {
            add_root(&mut roots, root);
        }
        let workspace_root = config.workspace_root.map(absolutize);
        let workspace_real = workspace_root
            .as_deref()
            .and_then(projected_canonical_root)
            .filter(|real| Some(real) != workspace_root.as_ref());
        Ok(Self {
            roots,
            workspace_root,
            workspace_real,
            home_dir: home,
        })
    }

    pub fn validate_read(&self, input: impl AsRef<Path>) -> Result<ValidatedPath, PolicyError> {
        self.validate_impl(input.as_ref(), true)
    }

    /// Validate an existing path while preserving the reference validator's
    /// file-or-directory behavior. Reads use `validate_read` to require a file.
    pub fn validate(&self, input: impl AsRef<Path>) -> Result<ValidatedPath, PolicyError> {
        self.validate_impl(input.as_ref(), false)
    }

    fn validate_impl(
        &self,
        input: &Path,
        require_regular: bool,
    ) -> Result<ValidatedPath, PolicyError> {
        if input.as_os_str().is_empty() || input.to_string_lossy().trim().is_empty() {
            return Err(PolicyError::new(
                PolicyErrorCode::EmptyPath,
                "Path cannot be empty",
            ));
        }
        let absolute = self.expand_and_resolve(input);
        match std::fs::canonicalize(&absolute) {
            Ok(real) => self.validate_resolved(input, &absolute, real, require_regular),
            Err(error) => {
                // Canonicalization can reveal whether an outside path exists.
                // Resolve only its nearest existing ancestor to decide the
                // policy boundary, then give the same denial for missing and
                // existing targets outside the allowed roots.
                let projected = projected_canonical_root(&absolute);
                if projected.as_ref().is_none_or(|path| !self.allowed(path)) {
                    return Err(PolicyError::new(
                        PolicyErrorCode::OutsideAllowedRoots,
                        format!(
                            "Path '{}' is outside allowed directories{}",
                            self.display_requested(input, &absolute),
                            self.describe_roots()
                        ),
                    )
                    .with_path(self.display_requested(input, &absolute)));
                }
                Err(self.io_error(error, input))
            }
        }
    }

    /// Discovery prunes denied subtrees. Let the walker account for ordinary
    /// filesystem errors itself so permission/not-found diagnostics stay intact.
    pub fn permits_discovery(&self, input: impl AsRef<Path>) -> bool {
        match self.validate(input) {
            Ok(_) => true,
            Err(error) => !matches!(
                error.code,
                PolicyErrorCode::IgnoredPath
                    | PolicyErrorCode::OutsideAllowedRoots
                    | PolicyErrorCode::SymlinkEscape
                    | PolicyErrorCode::SymlinkLoop
            ),
        }
    }

    /// Discovery decisions for one walk ([`DiscoveryWalk::permits`]), equal
    /// to [`Self::permits_discovery`] at a fraction of its cost.
    pub fn discovery_walk(&self) -> DiscoveryWalk<'_> {
        DiscoveryWalk {
            policy: self,
            parents: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn exists(&self, input: impl AsRef<Path>) -> bool {
        self.validate(input).is_ok()
    }

    pub fn allowed_roots(&self) -> Vec<PathBuf> {
        self.roots.clone()
    }

    /// Digest of the allowed roots: a result stored under one policy never
    /// serves a caller with other roots.
    pub fn identity(&self) -> String {
        crate::digest::json_sha256(
            &self
                .roots
                .iter()
                .map(|root| root.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
        )
    }

    pub fn validate_output(&self, input: impl AsRef<Path>) -> Result<ValidatedPath, PolicyError> {
        let input = input.as_ref();
        if input.as_os_str().is_empty() {
            return Err(PolicyError::new(
                PolicyErrorCode::EmptyPath,
                "Path cannot be empty",
            ));
        }
        let absolute = self.expand_and_resolve(input);
        if absolute.exists() {
            return self.validate_resolved(
                input,
                &absolute,
                std::fs::canonicalize(&absolute).map_err(|error| self.io_error(error, input))?,
                false,
            );
        }
        // Target does not exist yet. Walk up to the nearest existing ancestor,
        // recording the not-yet-created lexical tail. Canonicalize that ancestor
        // so any symlink in the parent chain is resolved *now* (TOCTOU), verify
        // the resolved parent still lives inside the allowed roots, then re-join
        // the tail so the returned path carries no unresolved symlink component.
        let mut ancestor = absolute.as_path();
        let mut tail: Vec<std::ffi::OsString> = Vec::new();
        while !ancestor.exists() {
            let name = ancestor.file_name().ok_or_else(|| {
                PolicyError::new(
                    PolicyErrorCode::NotFound,
                    format!(
                        "Path does not exist: {}",
                        self.display_requested(input, &absolute)
                    ),
                )
            })?;
            tail.push(name.to_os_string());
            ancestor = ancestor.parent().ok_or_else(|| {
                PolicyError::new(
                    PolicyErrorCode::NotFound,
                    format!(
                        "Path does not exist: {}",
                        self.display_requested(input, &absolute)
                    ),
                )
            })?;
        }
        let real_ancestor =
            std::fs::canonicalize(ancestor).map_err(|error| self.io_error(error, input))?;
        if !self.allowed(&real_ancestor) {
            return Err(PolicyError::new(
                PolicyErrorCode::OutsideAllowedRoots,
                format!(
                    "Path '{}' is outside allowed directories{}",
                    self.display_requested(input, &absolute),
                    self.describe_roots()
                ),
            )
            .with_path(self.display_requested(input, &absolute)));
        }
        let mut resolved = real_ancestor.clone();
        for name in tail.into_iter().rev() {
            resolved.push(name);
        }
        // Guard against the resolved target re-entering an allowed root only via
        // the symlinked parent: the canonicalized destination must itself remain
        // inside the roots.
        if !self.allowed(&resolved) {
            return Err(PolicyError::new(
                PolicyErrorCode::OutsideAllowedRoots,
                format!(
                    "Path '{}' is outside allowed directories{}",
                    self.display_requested(input, &absolute),
                    self.describe_roots()
                ),
            )
            .with_path(self.display_requested(input, &absolute)));
        }
        if let Some(sensitive) = [&absolute, &real_ancestor, &resolved]
            .into_iter()
            .find(|path| is_sensitive_path(path))
        {
            return Err(PolicyError::new(
                PolicyErrorCode::IgnoredPath,
                withheld_message(&self.redact(input), sensitive),
            )
            .with_path(self.redact(input)));
        }
        Ok(ValidatedPath {
            canonical: resolved.clone(),
            display: self.redact(&resolved),
        })
    }

    fn validate_resolved(
        &self,
        input: &Path,
        lexical: &Path,
        real: PathBuf,
        require_regular: bool,
    ) -> Result<ValidatedPath, PolicyError> {
        if !self.allowed(&real) {
            let lexical_allowed = self.allowed(lexical);
            let (code, message) = if lexical_allowed {
                (
                    PolicyErrorCode::SymlinkEscape,
                    format!(
                        "Symlink target '{}' is outside allowed directories{}",
                        self.redact(&real),
                        self.describe_roots()
                    ),
                )
            } else {
                (
                    PolicyErrorCode::OutsideAllowedRoots,
                    format!(
                        "Path '{}' is outside allowed directories{}",
                        self.display_requested(input, lexical),
                        self.describe_roots()
                    ),
                )
            };
            return Err(
                PolicyError::new(code, message).with_path(self.display_requested(input, lexical))
            );
        }
        if is_sensitive_path(lexical) {
            return Err(PolicyError::new(
                PolicyErrorCode::IgnoredPath,
                withheld_message(&self.redact(lexical), lexical),
            )
            .with_path(self.redact(lexical)));
        }
        if is_sensitive_path(&real) {
            return Err(PolicyError::new(
                PolicyErrorCode::IgnoredPath,
                format!(
                    "Symlink target: {}",
                    withheld_message(&self.redact(&real), &real)
                ),
            )
            .with_path(self.redact(&real)));
        }
        if require_regular
            && !std::fs::metadata(&real)
                .map_err(|error| self.io_error(error, &real))?
                .is_file()
        {
            return Err(PolicyError::new(
                PolicyErrorCode::NotRegular,
                format!("Path is not a regular file: {}", self.redact(&real)),
            )
            .with_path(self.redact(&real)));
        }
        Ok(ValidatedPath {
            canonical: real.clone(),
            display: self.redact(&real),
        })
    }

    fn allowed(&self, path: &Path) -> bool {
        self.roots
            .iter()
            .any(|root| path == root || path.starts_with(root))
    }

    pub(crate) fn expand_and_resolve(&self, input: &Path) -> PathBuf {
        let expanded = input.to_string_lossy();
        let path = if expanded == "~" {
            self.home_dir.clone().unwrap_or_else(|| PathBuf::from("~"))
        } else if let Some(tail) = expanded.strip_prefix("~/") {
            self.home_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(tail)
        } else {
            input.to_path_buf()
        };
        // Relative tool paths belong to the workspace, not to whatever
        // directory the host process happened to start in (WORKSPACE_ROOT
        // exists so a CLI run from /tmp still reads the configured project).
        match &self.workspace_root {
            Some(root) if path.is_relative() => normalize(&root.join(path)),
            _ => absolutize(path),
        }
    }

    /// `path` relative to the workspace root (`.` for the root itself), or
    /// `None` when it lies outside the workspace or none is configured.
    pub fn workspace_relative(&self, path: impl AsRef<Path>) -> Option<String> {
        let normalized = normalize(path.as_ref());
        let relative = [&self.workspace_root, &self.workspace_real]
            .into_iter()
            .flatten()
            .find_map(|root| normalized.strip_prefix(root).ok())?;
        Some(if relative.as_os_str().is_empty() {
            ".".to_owned()
        } else {
            relative.to_string_lossy().into_owned()
        })
    }

    pub fn redact(&self, path: impl AsRef<Path>) -> String {
        let normalized = normalize(path.as_ref());
        if let Some(relative) = self.workspace_relative(&normalized) {
            return relative;
        }
        if let Some(home) = &self.home_dir
            && let Ok(relative) = normalized.strip_prefix(home)
        {
            return if relative.as_os_str().is_empty() {
                "~".to_owned()
            } else {
                format!("~/{}", relative.to_string_lossy())
            };
        }
        normalized
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    }

    /// A denied path for public messages: workspace-relative when the
    /// expanded path lies in the workspace, otherwise the caller's literal
    /// input. Expansion joins relative input to the workspace root, so
    /// echoing the expanded form could add a prefix the caller never sent;
    /// canonical targets such as symlink destinations stay on `redact`.
    fn display_requested(&self, input: &Path, absolute: &Path) -> String {
        let normalized = normalize(absolute);
        if self
            .workspace_root
            .as_ref()
            .is_some_and(|root| normalized.starts_with(root))
        {
            self.redact(&normalized)
        } else {
            input.to_string_lossy().into_owned()
        }
    }

    fn describe_roots(&self) -> String {
        if self.roots.is_empty() {
            return String::new();
        }
        // Debug builds may surface the full filesystem layout to aid diagnosis.
        if cfg!(debug_assertions) {
            return format!(
                " (allowed: {})",
                self.roots
                    .iter()
                    .map(|root| root.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        // Release builds must not leak absolute filesystem paths in user-facing
        // errors; show only redacted/abbreviated roots.
        format!(
            " (allowed: {})",
            self.roots
                .iter()
                .map(|root| self.redact(root))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    fn io_error(&self, error: std::io::Error, input: &Path) -> PolicyError {
        let (code, prefix) = match error.kind() {
            std::io::ErrorKind::NotFound => (PolicyErrorCode::NotFound, "Path does not exist"),
            std::io::ErrorKind::PermissionDenied => (
                PolicyErrorCode::PermissionDenied,
                "Permission denied accessing path",
            ),
            _ if error.raw_os_error() == Some(40) || error.raw_os_error() == Some(62) => (
                PolicyErrorCode::SymlinkLoop,
                "Symlink loop detected at path",
            ),
            _ if error.raw_os_error() == Some(36) => {
                (PolicyErrorCode::NameTooLong, "Path name too long")
            }
            _ => (PolicyErrorCode::Io, "Unexpected error validating path"),
        };
        let shown = self.display_requested(input, &self.expand_and_resolve(input));
        PolicyError::new(code, format!("{prefix}: {shown}")).with_path(shown)
    }
}

fn add_root(roots: &mut Vec<PathBuf>, root: &Path) {
    let absolute = absolutize(root);
    if !roots.contains(&absolute) {
        roots.push(absolute.clone());
    }
    if let Some(real) = projected_canonical_root(&absolute)
        && !roots.contains(&real)
    {
        roots.push(real);
    }
}

/// Resolve an allowed root even when its final components do not exist yet.
/// The missing tail is lexical; any existing symlinked ancestor is resolved.
/// Once the root is created, validation still canonicalizes the actual target
/// and rejects a symlink that redirects it outside this projected root.
fn projected_canonical_root(root: &Path) -> Option<PathBuf> {
    let mut ancestor = root;
    let mut missing = Vec::new();
    while !ancestor.exists() {
        missing.push(ancestor.file_name()?.to_os_string());
        ancestor = ancestor.parent()?;
    }
    let mut projected = std::fs::canonicalize(ancestor).ok()?;
    for name in missing.into_iter().rev() {
        projected.push(name);
    }
    Some(projected)
}

fn absolutize(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    normalize(&joined)
}

fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    let portable = path.to_string_lossy().replace('\\', "/");
    for component in Path::new(&portable).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}

fn default_home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// [`PathPolicy::permits_discovery`] over one walk. An entry that is not a
/// symlink resolves to its parent directory's real path plus its own name,
/// so only each parent (once) and each symlink are canonicalized; the
/// decision then applies the same root and sensitive-path checks to the
/// same lexical and real paths.
pub struct DiscoveryWalk<'a> {
    policy: &'a PathPolicy,
    parents: std::sync::Mutex<std::collections::HashMap<PathBuf, Option<PathBuf>>>,
}

impl DiscoveryWalk<'_> {
    pub fn permits(&self, path: &Path) -> bool {
        let lexical = self.policy.expand_and_resolve(path);
        let (Some(parent), Some(name)) = (lexical.parent(), lexical.file_name()) else {
            return self.policy.permits_discovery(path);
        };
        let plain =
            std::fs::symlink_metadata(&lexical).is_ok_and(|meta| !meta.file_type().is_symlink());
        if !plain {
            return self.policy.permits_discovery(path);
        }
        let parent_real = {
            let mut parents = self
                .parents
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            parents
                .entry(parent.to_path_buf())
                .or_insert_with(|| std::fs::canonicalize(parent).ok())
                .clone()
        };
        let Some(parent_real) = parent_real else {
            return self.policy.permits_discovery(path);
        };
        let real = parent_real.join(name);
        self.policy.allowed(&real) && !is_sensitive_path(&lexical) && !is_sensitive_path(&real)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Callers own cleanup.
    fn fixture() -> PathBuf {
        tempfile::Builder::new()
            .prefix("octocode-policy-")
            .tempdir()
            .expect("path policy test setup should succeed")
            .keep()
    }

    /// A walk's discovery decisions equal `permits_discovery` for every
    /// entry: plain files, directories, sensitive names, and symlinks that
    /// stay inside or escape the allowed roots.
    #[cfg(unix)]
    #[test]
    fn walk_discovery_matches_permits_discovery_for_every_entry() {
        let root = fixture();
        let outside = fixture();
        std::fs::create_dir_all(root.join("src/nested")).expect("dirs");
        std::fs::write(root.join("src/lib.rs"), "x").expect("file");
        std::fs::write(root.join("src/nested/.env.production"), "x").expect("sensitive");
        std::fs::create_dir_all(root.join(".ssh")).expect("credential dir");
        std::fs::write(outside.join("secret.txt"), "x").expect("outside");
        std::os::unix::fs::symlink(outside.join("secret.txt"), root.join("src/escape"))
            .expect("escaping link");
        std::os::unix::fs::symlink(root.join("src/lib.rs"), root.join("src/inside"))
            .expect("inner link");
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.clone()),
            ..Default::default()
        })
        .expect("policy");
        let walk = policy.discovery_walk();
        for entry in [
            "src",
            "src/lib.rs",
            "src/nested",
            "src/nested/.env.production",
            ".ssh",
            "src/escape",
            "src/inside",
            "src/missing.rs",
        ] {
            let path = root.join(entry);
            assert_eq!(
                walk.permits(&path),
                policy.permits_discovery(&path),
                "{entry}"
            );
        }
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[test]
    fn newly_created_allowed_root_uses_its_resolved_parent_without_allowing_siblings() {
        let root = fixture();
        let allowed = root.join("new").join("home");
        let sibling = root.join("new").join("other");
        let policy = PathPolicy::new(PathPolicyConfig {
            additional_roots: vec![allowed.clone()],
            ..Default::default()
        })
        .expect("policy");
        assert!(!allowed.exists());
        std::fs::create_dir_all(&allowed).expect("new root");
        assert!(policy.validate_output(allowed.join("tmp/clone")).is_ok());
        assert!(policy.validate_output(&sibling).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn new_allowed_root_redirected_by_symlink_is_still_denied() {
        let root = fixture();
        let allowed = root.join("new").join("home");
        let outside = fixture();
        let policy = PathPolicy::new(PathPolicyConfig {
            additional_roots: vec![allowed.clone()],
            ..Default::default()
        })
        .expect("policy");
        std::fs::create_dir_all(allowed.parent().expect("parent")).expect("parent directory");
        std::os::unix::fs::symlink(&outside, &allowed).expect("redirect root");
        assert!(policy.validate_output(allowed.join("tmp/clone")).is_err());
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[test]
    fn permits_regular_workspace_files_and_redacts_the_workspace_prefix() {
        let root = fixture();
        let file = root.join("source.rs");
        std::fs::write(&file, "safe").expect("path policy test setup should succeed");
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.clone()),
            ..Default::default()
        })
        .expect("path policy test setup should succeed");
        let result = policy
            .validate_read(&file)
            .expect("path policy test setup should succeed");
        assert_eq!(
            result.canonical,
            std::fs::canonicalize(&file).expect("path policy test setup should succeed")
        );
        assert_eq!(result.display, "source.rs");
        assert_eq!(policy.redact(root.join("src\\sub\\..\\a.ts")), "src/a.ts");
        std::fs::remove_dir_all(root).expect("path policy test setup should succeed");
    }

    #[test]
    fn missing_paths_echo_the_requested_path_not_its_basename() {
        let root = fixture();
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.clone()),
            ..Default::default()
        })
        .expect("policy");
        for input in ["packages/nope/dir", "./packages/nope/dir"] {
            let error = policy.validate(input).expect_err("missing");
            assert_eq!(error.code, PolicyErrorCode::NotFound);
            assert_eq!(error.message, "Path does not exist: packages/nope/dir");
        }
        let absolute = root.join("gone/file.rs");
        let error = policy.validate(&absolute).expect_err("missing");
        assert_eq!(error.message, "Path does not exist: gone/file.rs");
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn rejects_sensitive_paths_and_directories_as_reads() {
        let root = fixture();
        let secret = root.join(".env");
        std::fs::write(&secret, "TOKEN=x").expect("path policy test setup should succeed");
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.clone()),
            ..Default::default()
        })
        .expect("path policy test setup should succeed");
        assert_eq!(
            policy
                .validate_read(secret)
                .expect_err("path policy should reject the test input")
                .code,
            PolicyErrorCode::IgnoredPath
        );
        assert_eq!(
            policy
                .validate_read(&root)
                .expect_err("path policy should reject the test input")
                .code,
            PolicyErrorCode::NotRegular
        );
        assert_eq!(
            policy
                .validate(&root)
                .expect("path policy test setup should succeed")
                .canonical,
            std::fs::canonicalize(&root).expect("path policy test setup should succeed")
        );
        assert_eq!(
            policy
                .validate("   ")
                .expect_err("path policy should reject the test input")
                .code,
            PolicyErrorCode::EmptyPath
        );
        std::fs::remove_dir_all(root).expect("path policy test setup should succeed");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape_and_output_under_symlinked_parent() {
        use std::os::unix::fs::symlink;

        let root = fixture();
        let outside = fixture();
        let outside_file = outside.join("secret.txt");
        std::fs::write(&outside_file, "secret").expect("path policy test setup should succeed");
        symlink(&outside_file, root.join("escape.txt"))
            .expect("path policy test setup should succeed");
        symlink(&outside, root.join("escape-dir")).expect("path policy test setup should succeed");
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.clone()),
            ..Default::default()
        })
        .expect("path policy test setup should succeed");
        assert_eq!(
            policy
                .validate_read(root.join("escape.txt"))
                .expect_err("path policy should reject the test input")
                .code,
            PolicyErrorCode::SymlinkEscape
        );
        assert_eq!(
            policy
                .validate_output(root.join("escape-dir/new.txt"))
                .expect_err("path policy should reject the test input")
                .code,
            PolicyErrorCode::OutsideAllowedRoots
        );
        std::fs::remove_dir_all(root).expect("path policy test setup should succeed");
        std::fs::remove_dir_all(outside).expect("path policy test setup should succeed");
    }

    #[test]
    fn home_is_not_allowed_unless_opted_in() {
        let workspace = fixture();
        let home = fixture();
        let outside = home.join("notes.txt");
        std::fs::write(&outside, "x").expect("path policy test setup should succeed");
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(workspace.clone()),
            home_dir: Some(home.clone()),
            include_home: false,
            ..Default::default()
        })
        .expect("path policy test setup should succeed");
        assert!(
            !policy
                .allowed_roots()
                .iter()
                .any(|root| home == *root || home.starts_with(root)),
            "home must not be a default allowed root"
        );
        assert_eq!(
            policy
                .validate_read(&outside)
                .expect_err("home read must be denied by default")
                .code,
            PolicyErrorCode::OutsideAllowedRoots
        );
        let opted = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(workspace.clone()),
            home_dir: Some(home.clone()),
            include_home: true,
            ..Default::default()
        })
        .expect("path policy test setup should succeed");
        assert!(
            opted.validate_read(&outside).is_ok(),
            "explicit include_home must grant home access"
        );
        std::fs::remove_dir_all(workspace).expect("path policy test setup should succeed");
        std::fs::remove_dir_all(home).expect("path policy test setup should succeed");
    }

    #[cfg(unix)]
    #[test]
    fn outside_denial_names_the_requested_path_but_never_a_symlink_target() {
        use std::os::unix::fs::symlink;

        let workspace = fixture();
        let outside = fixture();
        let requested = outside.join("app.ts");
        std::fs::write(&requested, "x").expect("write outside fixture");
        symlink(&requested, workspace.join("link.ts")).expect("symlink fixture");
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(workspace.clone()),
            ..Default::default()
        })
        .expect("policy");
        let denied = policy.validate_read(&requested).expect_err("outside path");
        assert!(
            denied
                .message
                .contains(&*normalize(&requested).to_string_lossy()),
            "{}",
            denied.message
        );
        let escaped = policy
            .validate_read(workspace.join("link.ts"))
            .expect_err("symlink escape");
        assert!(
            !escaped.message.contains(&*outside.to_string_lossy()),
            "{}",
            escaped.message
        );
        std::fs::remove_dir_all(workspace).expect("remove workspace fixture");
        std::fs::remove_dir_all(outside).expect("remove outside fixture");
    }

    #[test]
    fn relative_paths_resolve_against_the_workspace_root_not_the_process_cwd() {
        let workspace = fixture();
        std::fs::create_dir_all(workspace.join("src")).expect("src dir");
        std::fs::write(workspace.join("src/a.ts"), "x").expect("write fixture");
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(workspace.clone()),
            ..Default::default()
        })
        .expect("policy");
        let read = policy
            .validate_read("src/a.ts")
            .expect("relative path must resolve under WORKSPACE_ROOT");
        assert_eq!(
            read.canonical,
            std::fs::canonicalize(workspace.join("src/a.ts")).expect("canonical")
        );
        assert_eq!(read.display, "src/a.ts");
        let dir = policy.validate(".").expect("`.` is the workspace root");
        assert_eq!(
            dir.canonical,
            std::fs::canonicalize(&workspace).expect("canonical")
        );
        std::fs::remove_dir_all(workspace).expect("remove workspace fixture");
    }

    #[test]
    fn relative_outside_denial_echoes_only_the_caller_input() {
        let workspace = fixture();
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(workspace.join("nested")),
            ..Default::default()
        })
        .expect("policy");
        std::fs::create_dir_all(workspace.join("nested")).expect("nested root");
        let denied = policy
            .validate_read("../../outside.txt")
            .expect_err("outside path");
        assert!(
            denied.message.contains("'../../outside.txt'"),
            "{}",
            denied.message
        );
        let prefix = workspace
            .parent()
            .expect("parent")
            .to_string_lossy()
            .into_owned();
        assert!(
            !denied
                .message
                .split(" (allowed")
                .next()
                .unwrap_or_default()
                .contains(&prefix),
            "{}",
            denied.message
        );
        std::fs::remove_dir_all(workspace).expect("remove workspace fixture");
    }

    #[test]
    fn outside_read_denial_does_not_reveal_existence() {
        let workspace = fixture();
        let outside = fixture();
        let present = outside.join("present.txt");
        let absent = outside.join("absent.txt");
        std::fs::write(&present, "x").expect("write outside fixture");
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(workspace.clone()),
            ..Default::default()
        })
        .expect("policy");
        for path in [present, absent] {
            assert_eq!(
                policy.validate_read(path).expect_err("outside path").code,
                PolicyErrorCode::OutsideAllowedRoots
            );
        }
        std::fs::remove_dir_all(workspace).expect("remove workspace fixture");
        std::fs::remove_dir_all(outside).expect("remove outside fixture");
    }

    #[test]
    fn additional_roots_are_allowed_and_builtin_ignores_still_apply() {
        // `local.allowedPaths` reaches the policy as `additional_roots`.
        let workspace = fixture();
        let extra = fixture();
        let visible = extra.join("notes.txt");
        let sensitive = extra.join("terraform.tfstate");
        std::fs::write(&visible, "value").expect("write fixture");
        std::fs::write(&sensitive, "value").expect("write fixture");
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(workspace.clone()),
            additional_roots: vec![extra.clone()],
            ..Default::default()
        })
        .expect("policy");
        assert!(policy.validate_read(&visible).is_ok());
        assert_eq!(
            policy
                .validate_read(&sensitive)
                .expect_err("built-in ignore must apply under additional roots")
                .code,
            PolicyErrorCode::IgnoredPath
        );
        std::fs::remove_dir_all(workspace).expect("remove fixture");
        std::fs::remove_dir_all(extra).expect("remove fixture");
    }
}
