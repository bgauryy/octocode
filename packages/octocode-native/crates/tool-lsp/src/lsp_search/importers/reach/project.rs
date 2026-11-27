//! Where the project places a bare module specifier: the graph's own
//! resolution (`import_resolution::aliases`: tsconfig `paths` and `baseUrl`,
//! workspace packages, package `imports`) over the files the request's text
//! scan listed under the scope, loaded once per request when a bare
//! specifier first asks.
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::security::ContentSecurity;
use octocode_tools_shared::import_resolution::aliases::{BareTarget, ResolveContext};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

/// The module a bare specifier names, as far as the project's settings say.
pub(super) enum Placed {
    /// A listed file of the scope (absolute).
    At(PathBuf),
    /// No project code: a builtin or an installed package.
    Elsewhere,
    /// Project code no listed file was found for (an alias or workspace
    /// package the listing does not hold), or an importer outside the scope.
    Unknown,
}

pub(super) struct Project {
    /// Canonical scope root.
    root: PathBuf,
    /// The listed files and the anchor, root-relative with `/`.
    known: BTreeSet<String>,
    policy: PathPolicy,
    context: OnceLock<ResolveContext>,
}

impl std::fmt::Debug for Project {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Project")
            .field("root", &self.root)
            .field("known", &self.known.len())
            .finish_non_exhaustive()
    }
}

impl Project {
    /// The project of the scope `root` (canonical) whose text scan `listed`
    /// these files; `anchor` is known too, wherever the scan's ignore rules
    /// left it. Configuration is read under `policy`.
    pub(super) fn new(root: &Path, listed: &[PathBuf], anchor: &Path, policy: &PathPolicy) -> Self {
        Self {
            root: root.to_path_buf(),
            known: listed
                .iter()
                .map(PathBuf::as_path)
                .chain(std::iter::once(anchor))
                .filter_map(|file| relative(root, file))
                .collect(),
            policy: policy.clone(),
            context: OnceLock::new(),
        }
    }

    /// Where the bare `specifier` of `file` (canonical) leads.
    pub(super) fn place(&self, specifier: &str, file: &str) -> Placed {
        let Some(importer) = relative(&self.root, Path::new(file)) else {
            return Placed::Unknown;
        };
        let context = self.context.get_or_init(|| {
            ResolveContext::load(
                &self.root,
                &self.known,
                &self.policy,
                &ContentSecurity::new(),
            )
        });
        match context.resolve_bare_js(specifier, &importer, &self.known) {
            Some(target) => Placed::At(self.root.join(target)),
            // Project code the resolution names without finding its file.
            None if context.bare_target_js(specifier, &importer) != BareTarget::External => {
                Placed::Unknown
            }
            None => Placed::Elsewhere,
        }
    }
}

/// `file` below `root` as the resolution spells paths: root-relative,
/// `/`-separated. `None` outside the root or for a name that is not UTF-8.
fn relative(root: &Path, file: &Path) -> Option<String> {
    let parts = file
        .strip_prefix(root)
        .ok()?
        .components()
        .map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    (!parts.is_empty()).then(|| parts.join("/"))
}
