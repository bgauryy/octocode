use std::path::{Component, Path, PathBuf};

use regex::Regex;

use super::discovery::is_sensitive_path;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathType {
    File,
    Directory,
    Symlink,
}

#[derive(Clone, Debug)]
pub struct PathPolicy {
    roots: Vec<PathBuf>,
    workspace_root: Option<PathBuf>,
    home_dir: Option<PathBuf>,
    ignored_path_patterns: Vec<Regex>,
    ignored_file_patterns: Vec<Regex>,
}

impl PathPolicy {
    pub fn new(config: PathPolicyConfig) -> Result<Self, PolicyError> {
        Self::build(config, &[], &[], &[])
    }

    pub fn with_registry(
        config: PathPolicyConfig,
        registry: &crate::security::SecurityRegistry,
    ) -> Result<Self, PolicyError> {
        Self::build(
            config,
            registry.allowed_roots(),
            registry.ignored_path_patterns(),
            registry.ignored_file_patterns(),
        )
    }

    fn build(
        config: PathPolicyConfig,
        registry_roots: &[PathBuf],
        ignored_path_patterns: &[Regex],
        ignored_file_patterns: &[Regex],
    ) -> Result<Self, PolicyError> {
        let home = config.home_dir.or_else(default_home);
        let mut roots = Vec::new();
        for root in config
            .workspace_root
            .iter()
            .chain(config.additional_roots.iter())
            .chain(registry_roots.iter())
            .chain(config.include_home.then_some(home.as_ref()).flatten())
        {
            add_root(&mut roots, root);
        }
        Ok(Self {
            roots,
            workspace_root: config.workspace_root.map(absolutize),
            home_dir: home,
            ignored_path_patterns: ignored_path_patterns.to_vec(),
            ignored_file_patterns: ignored_file_patterns.to_vec(),
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
            Ok(real) => self.validate_resolved(&absolute, real, require_regular),
            Err(error) => Err(self.io_error(error, input)),
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

    pub fn exists(&self, input: impl AsRef<Path>) -> bool {
        self.validate(input).is_ok()
    }

    pub fn get_type(&self, input: impl AsRef<Path>) -> Option<PathType> {
        let lexical = self.expand_and_resolve(input.as_ref());
        self.validate(&lexical).ok()?;
        let metadata = std::fs::symlink_metadata(lexical).ok()?;
        Some(if metadata.file_type().is_symlink() {
            PathType::Symlink
        } else if metadata.is_dir() {
            PathType::Directory
        } else {
            PathType::File
        })
    }

    pub fn allowed_roots(&self) -> Vec<PathBuf> {
        self.roots.clone()
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
                &absolute,
                std::fs::canonicalize(&absolute).map_err(|error| self.io_error(error, input))?,
                false,
            );
        }
        let mut ancestor = absolute.as_path();
        while !ancestor.exists() {
            ancestor = ancestor.parent().ok_or_else(|| {
                PolicyError::new(
                    PolicyErrorCode::NotFound,
                    format!("Path does not exist: {}", self.redact(input)),
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
                    self.redact(&absolute),
                    self.describe_roots()
                ),
            )
            .with_path(self.redact(&absolute)));
        }
        if self.ignored(&absolute) || self.ignored(&real_ancestor) {
            return Err(PolicyError::new(
                PolicyErrorCode::IgnoredPath,
                format!(
                    "Path '{}' is in an ignored directory or matches an ignored pattern",
                    self.redact(input)
                ),
            )
            .with_path(self.redact(input)));
        }
        Ok(ValidatedPath {
            canonical: absolute.clone(),
            display: self.redact(&absolute),
        })
    }

    fn validate_resolved(
        &self,
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
                        self.redact(lexical),
                        self.describe_roots()
                    ),
                )
            };
            return Err(PolicyError::new(code, message).with_path(self.redact(lexical)));
        }
        if self.ignored(lexical) {
            return Err(PolicyError::new(
                PolicyErrorCode::IgnoredPath,
                format!(
                    "Path '{}' is in an ignored directory or matches an ignored pattern",
                    self.redact(lexical)
                ),
            )
            .with_path(self.redact(lexical)));
        }
        if self.ignored(&real) {
            return Err(PolicyError::new(
                PolicyErrorCode::IgnoredPath,
                format!(
                    "Symlink target '{}' is in an ignored directory or matches an ignored pattern",
                    self.redact(&real)
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

    fn ignored(&self, path: &Path) -> bool {
        if is_sensitive_path(path) {
            return true;
        }
        let text = path.to_string_lossy();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        self.ignored_path_patterns
            .iter()
            .any(|pattern| pattern.is_match(&text))
            || self
                .ignored_file_patterns
                .iter()
                .any(|pattern| pattern.is_match(&name))
    }

    fn expand_and_resolve(&self, input: &Path) -> PathBuf {
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
        absolutize(path)
    }

    pub fn redact(&self, path: impl AsRef<Path>) -> String {
        let normalized = normalize(path.as_ref());
        if let Some(root) = &self.workspace_root
            && let Ok(relative) = normalized.strip_prefix(root)
        {
            return if relative.as_os_str().is_empty() {
                ".".to_owned()
            } else {
                relative.to_string_lossy().into_owned()
            };
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

    fn describe_roots(&self) -> String {
        if self.roots.is_empty() {
            String::new()
        } else {
            format!(
                " (allowed: {})",
                self.roots
                    .iter()
                    .map(|root| root.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
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
        PolicyError::new(code, format!("{prefix}: {}", self.redact(input)))
            .with_path(self.redact(input))
    }
}

fn add_root(roots: &mut Vec<PathBuf>, root: &Path) {
    let absolute = absolutize(root);
    if !roots.contains(&absolute) {
        roots.push(absolute.clone());
    }
    if let Ok(real) = std::fs::canonicalize(absolute)
        && !roots.contains(&real)
    {
        roots.push(real);
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn fixture() -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("path policy test setup should succeed")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "octocode-policy-{}-{id}-{}",
            std::process::id(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).expect("path policy test setup should succeed");
        root
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
    fn registry_roots_and_ignore_patterns_change_enforcement() {
        let root = fixture();
        let file = root.join("generated.locked");
        std::fs::write(&file, "value").expect("write fixture");
        let mut registry = crate::security::SecurityRegistry::default();
        registry
            .add_allowed_roots([root.clone()])
            .expect("add root");
        registry
            .add_ignored_file_patterns([Regex::new(r"\.locked$").expect("regex")])
            .expect("add ignore");
        let policy = PathPolicy::with_registry(PathPolicyConfig::default(), &registry)
            .expect("registry policy");
        assert_eq!(
            policy
                .validate_read(file)
                .expect_err("custom ignore must apply")
                .code,
            PolicyErrorCode::IgnoredPath
        );
        std::fs::remove_dir_all(root).expect("remove fixture");
    }
}
