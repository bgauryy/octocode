use octocode_engine::text::lexical;
use std::path::{Path, PathBuf};

use super::discovery::{EntryKind, withheld_link_message, withheld_message};
use super::{PolicyError, PolicyErrorCode};

mod credentials;
mod messages;
mod output;
#[cfg(test)]
mod tests;
mod walk;

pub use credentials::withheld_text_by_default;
pub use messages::repeated_folder_note;
pub use walk::DiscoveryWalk;

#[derive(Clone, Debug, Default)]
pub struct PathPolicyConfig {
    pub workspace_root: Option<PathBuf>,
    pub additional_roots: Vec<PathBuf>,
    /// Allowed roots that store one tree per directory `depth` levels below
    /// them (clones, materialized snapshots, URL captures): a path under one
    /// belongs to its own tree, never to the trees beside it.
    pub tree_stores: Vec<(PathBuf, usize)>,
    /// Roots Octocode writes its own outputs under and reads them back
    /// from ([`PathPolicy::validate_output`]), such as the graph snapshot
    /// home: never read roots of the local tools.
    pub output_roots: Vec<PathBuf>,
    pub include_home: bool,
    /// The user's home, used for `~` expansion, `~/` redaction and
    /// `include_home`; `None` turns all three off (the runtime always sets it).
    pub home_dir: Option<PathBuf>,
    /// `security.credentialFiles: allow`: credential file names are not
    /// withheld, except [`Self::octocode_home`] and files directly in `home_dir`.
    pub allow_credential_files: bool,
    /// Holds Octocode's own credentials, whatever the setting.
    pub octocode_home: Option<PathBuf>,
    /// The global ignore rules the call's walks apply, resolved from the
    /// call's own environment (the default reads the process's).
    pub global_ignore: octocode_engine::search::walk::GlobalIgnore,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedPath {
    pub canonical: PathBuf,
    pub display: String,
}

#[derive(Clone, Debug)]
pub struct PathPolicy {
    roots: Vec<PathBuf>,
    /// Directories no read admits though an allowed root contains them
    /// ([`Self::closing`]), lexical and resolved.
    closed_roots: Vec<PathBuf>,
    /// [`PathPolicyConfig::output_roots`], lexical and resolved.
    output_roots: Vec<PathBuf>,
    /// [`PathPolicyConfig::tree_stores`], lexical and resolved.
    tree_stores: Vec<(PathBuf, usize)>,
    workspace_root: Option<PathBuf>,
    /// The workspace root with symlinks resolved (e.g. macOS `/var` →
    /// `/private/var`), so canonical paths still display workspace-relative.
    workspace_real: Option<PathBuf>,
    home_dir: Option<PathBuf>,
    allow_credential_files: bool,
    /// The Octocode home, lexical and resolved.
    octocode_home: Vec<PathBuf>,
    /// The user's home, lexical and resolved.
    home_forms: Vec<PathBuf>,
    global_ignore: octocode_engine::search::walk::GlobalIgnore,
}

impl PathPolicy {
    pub fn new(config: PathPolicyConfig) -> Result<Self, PolicyError> {
        let home = config.home_dir;
        let mut roots = Vec::new();
        for root in config
            .workspace_root
            .iter()
            .chain(config.additional_roots.iter())
            .chain(config.include_home.then_some(home.as_ref()).flatten())
        {
            add_root(&mut roots, root);
        }
        let mut output_roots = Vec::new();
        for root in &config.output_roots {
            add_root(&mut output_roots, root);
        }
        let mut tree_stores = Vec::new();
        for (store, depth) in &config.tree_stores {
            let mut forms = Vec::new();
            add_root(&mut forms, store);
            tree_stores.extend(forms.into_iter().map(|form| (form, *depth)));
        }
        let workspace_root = config.workspace_root.map(absolutize);
        let workspace_real = workspace_root
            .as_deref()
            .and_then(projected_canonical_root)
            .filter(|real| Some(real) != workspace_root.as_ref());
        let forms = |home: Option<PathBuf>| -> Vec<PathBuf> {
            home.map(absolutize)
                .into_iter()
                .flat_map(|home| {
                    let real = projected_canonical_root(&home);
                    std::iter::once(home).chain(real)
                })
                .collect()
        };
        let octocode_home = forms(config.octocode_home);
        let home_forms = forms(home.clone());
        Ok(Self {
            roots,
            closed_roots: Vec::new(),
            output_roots,
            tree_stores,
            workspace_root,
            workspace_real,
            home_dir: home,
            allow_credential_files: config.allow_credential_files,
            octocode_home,
            home_forms,
            global_ignore: config.global_ignore,
        })
    }

    /// The global ignore rules every walk of this call applies.
    pub fn global_ignore(&self) -> octocode_engine::search::walk::GlobalIgnore {
        self.global_ignore
    }

    /// This policy with `root` closed: nothing under it is read, even inside
    /// an allowed root (a workspace that contains the Octocode home must not
    /// open its stores, whose files are no checkout).
    pub fn closing(mut self, root: &Path) -> Self {
        add_root(&mut self.closed_roots, root);
        self
    }

    pub fn validate_read(&self, input: impl AsRef<Path>) -> Result<ValidatedPath, PolicyError> {
        self.validate_impl(input.as_ref(), Access::RegularFile)
    }

    /// Validate an existing path while preserving the reference validator's
    /// file-or-directory behavior. Reads use `validate_read` to require a file.
    pub fn validate(&self, input: impl AsRef<Path>) -> Result<ValidatedPath, PolicyError> {
        self.validate_impl(input.as_ref(), Access::Read)
    }

    fn validate_impl(&self, input: &Path, access: Access) -> Result<ValidatedPath, PolicyError> {
        if input.as_os_str().is_empty() || input.to_string_lossy().trim().is_empty() {
            return Err(PolicyError::new(
                PolicyErrorCode::EmptyPath,
                "Path cannot be empty",
            ));
        }
        reject_nul(input)?;
        let absolute = self.expand_and_resolve(input);
        match std::fs::canonicalize(&absolute) {
            Ok(real) => self.validate_resolved(input, &absolute, real, access),
            Err(error) => {
                // Canonicalization can reveal whether an outside path exists.
                // Resolve only its nearest existing ancestor to decide the
                // policy boundary, then give the same denial for missing and
                // existing targets outside the allowed roots.
                let projected = projected_canonical_root(&absolute);
                if projected.as_ref().is_none_or(|path| !self.allowed(path)) {
                    return Err(PolicyError::new(
                        PolicyErrorCode::OutsideAllowedRoots,
                        self.outside_roots(input, &absolute),
                    )
                    .with_path(self.display_requested(input, &absolute)));
                }
                if ambiguous_redacted_name(&absolute) {
                    return Err(PolicyError::new(
                        PolicyErrorCode::NotFound,
                        format!(
                            "Path {} names a file by its redacted name, and several entries show that name: no one of them can be chosen from it.",
                            self.display_requested(input, &absolute)
                        ),
                    )
                    .with_path(self.display_requested(input, &absolute)));
                }
                Err(self.io_error(error, input))
            }
        }
    }

    /// Whether `input` names a file by a redacted (shown) name that several
    /// entries share: the one fact behind the policy's "several entries show
    /// that name" answer, asked of the policy itself so no caller reads the
    /// answer's wording.
    pub fn names_several_entries(&self, input: &Path) -> bool {
        ambiguous_redacted_name(&self.expand_and_resolve(input))
    }

    pub fn allowed_roots(&self) -> Vec<PathBuf> {
        self.roots.clone()
    }

    /// Digest of the allowed roots and output roots: a result stored under
    /// one policy never serves a caller with other roots.
    pub fn identity(&self) -> String {
        crate::digest::json_sha256(
            &self
                .roots
                .iter()
                .chain(&self.output_roots)
                .chain(&self.closed_roots)
                .map(|root| root.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
        )
    }

    fn validate_resolved(
        &self,
        input: &Path,
        lexical: &Path,
        real: PathBuf,
        access: Access,
    ) -> Result<ValidatedPath, PolicyError> {
        if !self.admits(access, &real) {
            let lexical_allowed = self.admits(access, lexical);
            let (code, message) = if lexical_allowed {
                (
                    PolicyErrorCode::SymlinkEscape,
                    format!(
                        "Symlink '{}' resolves to {}, outside allowed directories.{}",
                        self.display_requested(input, lexical),
                        self.describe_target(&real),
                        self.describe_roots()
                    ),
                )
            } else {
                (
                    PolicyErrorCode::OutsideAllowedRoots,
                    self.outside_roots(input, lexical),
                )
            };
            return Err(
                PolicyError::new(code, message).with_path(self.display_requested(input, lexical))
            );
        }
        // Read time re-checks with the kind on disk (fail-closed when unknown).
        if self.withholds(lexical, EntryKind::stat(lexical)) {
            return Err(PolicyError::new(
                PolicyErrorCode::IgnoredPath,
                withheld_message(&self.redact(lexical), lexical),
            )
            .with_path(self.redact(lexical)));
        }
        if self.withholds(&real, EntryKind::stat(&real)) {
            // Named by the link the caller asked for, never by the target.
            return Err(PolicyError::new(
                PolicyErrorCode::IgnoredPath,
                withheld_link_message(&self.redact(lexical), &real),
            )
            .with_path(self.redact(lexical)));
        }
        if access == Access::RegularFile
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
        within(&self.roots, path) && !within(&self.closed_roots, path)
    }

    /// Whether `access` admits `path`: an output may also lie under an
    /// output root; anything else only under the allowed roots.
    fn admits(&self, access: Access, path: &Path) -> bool {
        self.allowed(path) || (access == Access::Output && within(&self.output_roots, path))
    }

    pub fn expand_and_resolve(&self, input: &Path) -> PathBuf {
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
        let resolved = match &self.workspace_root {
            Some(root) if path.is_relative() => lexical(&root.join(path)),
            _ => absolutize(path),
        };
        resolve_redacted_names(resolved)
    }

    /// The top of the tree `path` lies in, where a search widened from it
    /// stops: the innermost of the workspace root, the one tree of a tree
    /// store holding it (a clone's checkout, never the store's other
    /// checkouts) and the allowed roots holding it. `None` outside them all,
    /// or between a tree store and its trees.
    pub fn tree_root(&self, path: impl AsRef<Path>) -> Option<PathBuf> {
        let path = lexical(path.as_ref());
        let holds = |root: &PathBuf| path.starts_with(root);
        let stored = self.tree_stores.iter().find(|(store, _)| holds(store));
        if let Some((store, depth)) = stored {
            let tree: PathBuf = path
                .strip_prefix(store)
                .ok()?
                .components()
                .take(*depth)
                .collect();
            if tree.components().count() < *depth {
                return None;
            }
            let tree = store.join(tree);
            // A workspace inside one checkout bounds the tree in turn.
            return [&self.workspace_root, &self.workspace_real]
                .into_iter()
                .flatten()
                .filter(|root| holds(root) && root.starts_with(&tree))
                .chain(std::iter::once(&tree))
                .max_by_key(|root| root.components().count())
                .cloned();
        }
        [&self.workspace_root, &self.workspace_real]
            .into_iter()
            .flatten()
            .chain(&self.roots)
            .filter(|root| holds(root))
            .max_by_key(|root| root.components().count())
            .cloned()
    }

    /// `path` as rows name it: workspace relative inside the workspace,
    /// absolute outside it.
    pub fn shown(&self, path: impl AsRef<Path>) -> String {
        self.workspace_relative(path.as_ref())
            .unwrap_or_else(|| lexical(path.as_ref()).to_string_lossy().into_owned())
    }

    /// `path` relative to the workspace root (`.` for the root itself), or
    /// `None` when it lies outside the workspace or none is configured.
    pub fn workspace_relative(&self, path: impl AsRef<Path>) -> Option<String> {
        let normalized = lexical(path.as_ref());
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

    /// `text` with the workspace root dropped from every path it names, so a
    /// message shows a file as a result row does (`src/a.rs`, not the host
    /// layout). Paths outside the workspace stay as they are, as rows show them.
    pub fn workspace_relative_text(&self, text: &str) -> String {
        let mut shown = text.to_owned();
        for root in [&self.workspace_root, &self.workspace_real]
            .into_iter()
            .flatten()
        {
            let prefix = format!("{}{}", root.display(), std::path::MAIN_SEPARATOR);
            // Only a path that starts here: `/var/w/` inside `/private/var/w/`
            // is the tail of another path (a symlinked root's real one).
            let mut parts = shown.split(prefix.as_str());
            let mut kept = parts.next().unwrap_or_default().to_owned();
            for part in parts {
                let starts_a_path = kept.chars().next_back().is_none_or(|before| {
                    before.is_whitespace() || matches!(before, '\'' | '"' | '(' | ':' | '`')
                });
                if !starts_a_path {
                    kept.push_str(&prefix);
                }
                kept.push_str(part);
            }
            shown = kept;
        }
        shown
    }

    /// The closest ancestor of `requested` the policy admits as a
    /// directory, named relative to the workspace when it lies inside it;
    /// `None` when no admitted ancestor exists (the climb never leaves the
    /// allowed roots).
    pub fn nearest_existing_dir(&self, requested: &str) -> Option<String> {
        let mut path = self.expand_and_resolve(Path::new(requested));
        while let Some(parent) = path.parent().map(Path::to_path_buf) {
            if let Ok(valid) = self.validate(&parent)
                && valid.canonical.is_dir()
            {
                return Some(
                    self.workspace_relative(&valid.canonical)
                        .unwrap_or_else(|| valid.canonical.to_string_lossy().into_owned()),
                );
            }
            path = parent;
        }
        None
    }

    pub fn redact(&self, path: impl AsRef<Path>) -> String {
        let normalized = lexical(path.as_ref());
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
}

/// What a path is validated for: which roots admit it, and what it must be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Access {
    /// Any existing entry under the allowed roots.
    Read,
    /// A regular file under the allowed roots.
    RegularFile,
    /// An entry Octocode writes, or reads back where it wrote it: under the
    /// allowed roots or an output root.
    Output,
}

/// Whether a component of `path` that holds a redaction placeholder, and names
/// no entry as sent, is the shown name of several entries.
fn ambiguous_redacted_name(path: &Path) -> bool {
    let placeholders = octocode_engine::security::placeholder_regex();
    let mut at = PathBuf::new();
    for part in path.components() {
        let name = part.as_os_str().to_string_lossy();
        let shown = at.join(part.as_os_str());
        if placeholders.is_match(&name) && shown.symlink_metadata().is_err() {
            return redacted_name_matches(&at, &name) > 1;
        }
        at = shown;
    }
    false
}

/// The entries of `dir` whose redacted name is `shown`.
fn redacted_name_matches(dir: &Path, shown: &str) -> usize {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            let entry = entry.file_name().to_string_lossy().into_owned();
            octocode_engine::security::sanitizer::sanitize_leaf(&entry)
                .ok()
                .flatten()
                .is_some_and(|redacted| redacted == shown)
        })
        .count()
}

/// `path` with each component that holds a redaction placeholder resolved to
/// the entry it stands for: a listing shows a file named after a credential
/// with the placeholder in its place (`deploy-[REDACTED-GITHUBTOKENS].txt`),
/// and that shown name is what a caller sends back. The entry is the one
/// sibling whose redacted name equals the component; with none, or several,
/// the component stays as sent and the path is an ordinary missing one.
fn resolve_redacted_names(path: PathBuf) -> PathBuf {
    let placeholders = octocode_engine::security::placeholder_regex();
    if !path
        .components()
        .any(|part| placeholders.is_match(&part.as_os_str().to_string_lossy()))
    {
        return path;
    }
    let mut resolved = PathBuf::new();
    for part in path.components() {
        let name = part.as_os_str().to_string_lossy();
        let shown = resolved.join(part.as_os_str());
        if !placeholders.is_match(&name) || shown.symlink_metadata().is_ok() {
            resolved = shown;
            continue;
        }
        let mut named = std::fs::read_dir(&resolved)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.file_name())
            .filter(|entry| {
                let entry = entry.to_string_lossy();
                octocode_engine::security::sanitizer::sanitize_leaf(&entry)
                    .ok()
                    .flatten()
                    .is_some_and(|redacted| redacted == name)
            });
        resolved = match (named.next(), named.next()) {
            (Some(only), None) => resolved.join(only),
            _ => shown,
        };
    }
    resolved
}

fn within(roots: &[PathBuf], path: &Path) -> bool {
    roots
        .iter()
        .any(|root| path == root || path.starts_with(root))
}

/// A NUL byte ends a path at the operating system: no file has that name,
/// so the request is malformed, not a filesystem failure.
fn reject_nul(input: &Path) -> Result<(), PolicyError> {
    if input.as_os_str().as_encoded_bytes().contains(&0) {
        return Err(PolicyError::new(
            PolicyErrorCode::InvalidInput,
            "Path contains a NUL byte; send the path without it",
        ));
    }
    Ok(())
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
    lexical(&joined)
}
