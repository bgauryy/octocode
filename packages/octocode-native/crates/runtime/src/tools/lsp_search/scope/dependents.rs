//! Which other projects under the scope depend on the anchor's project, read
//! from their build files. Only those can use the anchor's declarations: another
//! project's spelling of the name is a symbol of its own (guava's `android/`
//! flavour declares its own copy). A project whose build file names its
//! dependencies in a way this does not read counts as dependent, so an answer
//! that may miss it stays partial and names it.
//!
//! The read walks the whole scope, so it is kept across requests with a proof
//! of what it read: each directory the walk entered and the ignore files
//! there (a build file created, removed or renamed changes its directory's
//! stamp), each build file it read, the anchor's own, and the directories
//! above the scope a project root may widen to. A read whose inputs were
//! written within the settle window is not kept ([`Watch::capture`]).
use super::*;
use crate::tools::page_memo::{
    PageMemo, Proven, Watch, directory_ignore_inputs, repository_markers,
};
use octocode_engine::stamp::Link;

mod manifests;
use manifests::{Identity, depends_on};

/// The other projects under the scope that depend on the anchor's project, or
/// may: their roots, canonical.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in super::super) struct Dependents {
    /// `None`: the anchor's own project could not be named, so every other
    /// project may depend on it.
    roots: Option<BTreeSet<PathBuf>>,
}

impl Dependents {
    /// Whether the project rooted at `root` (canonical) may use the anchor.
    pub(in super::super) fn includes(&self, root: &Path) -> bool {
        self.roots.as_ref().is_none_or(|roots| roots.contains(root))
    }

    /// Whether no other project can use the anchor.
    pub(in super::super) fn is_empty(&self) -> bool {
        self.roots.as_ref().is_some_and(BTreeSet::is_empty)
    }
}

impl Scope {
    /// [`Dependents`] of the project the server at `server_root` answers for,
    /// whose anchor is `anchor`, read once per request on a cancellable worker.
    pub(in super::super) async fn dependents(
        &self,
        server_root: &str,
        anchor: &str,
        extension: &str,
        cancel: &dyn CancellationCheck,
    ) -> Result<&Dependents, LspFailure> {
        if let Some(known) = self.dependents.get() {
            return Ok(known);
        }
        let (root, server_root, anchor, extension) = (
            PathBuf::from(&self.root),
            canonical_path(server_root),
            PathBuf::from(anchor),
            extension.to_owned(),
        );
        let found = blocking_cancellable(cancel, move |stopped| {
            kept_or_read(&root, &server_root, &anchor, &extension, stopped)
        })
        .await?
        .flatten();
        // A stopped read is the request's end, not an answer.
        if found.is_none() {
            cancel.check().map_err(LspFailure::cancelled)?;
        }
        Ok(self.dependents.get_or_init(|| found.unwrap_or_default()))
    }

    /// [`Self::dependents`] once read.
    pub(in super::super) fn known_dependents(&self) -> Option<&Dependents> {
        self.dependents.get()
    }
}

fn canonical_path(path: &str) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.into())
}

/// Reads kept by scope root, server root, anchor directory and extension;
/// each says what it read, so a later request serves it while that holds.
static KEPT: PageMemo<Proven<Dependents>> = PageMemo::new();

/// The key already says what a read depends on.
const ANY_POLICY: &str = "";

/// [`read_dependents`], from [`KEPT`] while its proof holds; a read the
/// request stopped is not kept.
fn kept_or_read(
    root: &Path,
    server_root: &Path,
    anchor: &Path,
    extension: &str,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Option<Dependents> {
    let key = [
        root,
        server_root,
        anchor.parent().unwrap_or(anchor),
        Path::new(extension),
    ]
    .map(|part| part.to_string_lossy().into_owned())
    .join("\u{0}");
    if let Some(kept) = KEPT.current(&key, ANY_POLICY) {
        return Some(Dependents::clone(&kept));
    }
    #[cfg(test)]
    reads::note(root);
    // Before the walk's first read, on the request's own clock.
    let started = octocode_engine::stamp::settle_clock();
    let (found, read) = read_dependents(root, server_root, anchor, extension, stopped)?;
    if let Some(proof) = read.and_then(|read| read.proof(root, started)) {
        let bytes = std::mem::size_of::<Dependents>()
            + found
                .roots
                .iter()
                .flatten()
                .map(|root| path_bytes(root))
                .sum::<usize>()
            + proof.weight();
        KEPT.put_proven(
            key,
            ANY_POLICY.to_owned(),
            bytes,
            proof,
            Arc::new(found.clone()),
        );
    }
    Some(found)
}

fn path_bytes(path: &Path) -> usize {
    std::mem::size_of::<PathBuf>() + path.as_os_str().len()
}

/// What one read of the dependents depended on.
#[derive(Default)]
struct Read {
    /// The walked directories, their ignore files, the build files read and
    /// the anchor's own build file.
    watched: Vec<PathBuf>,
    /// Ignore inputs a nested repository reads from outside the walk.
    rules: Vec<PathBuf>,
    /// An ignore input no stamp can watch: the read is not kept.
    untracked: bool,
}

impl Read {
    fn enter(&mut self, dir: &Path, root: &Path) {
        // The walk reads every ignore file it finds; none is withheld here.
        let inputs = directory_ignore_inputs(dir, root, |_| true);
        self.watched.push(dir.to_path_buf());
        self.watched.extend(inputs.files);
        self.rules.extend(inputs.external);
        self.untracked |= inputs.untracked;
    }

    /// The proof of this read of `root`, when nothing it depends on was
    /// written within the settle window before `started`. A project root
    /// may widen above the scope up to the repository's root (outside a
    /// repository, to any ancestor), so the directories between are stamped
    /// too; a scope that is its repository's root widens nowhere.
    fn proof(
        mut self,
        root: &Path,
        started: std::time::SystemTime,
    ) -> Option<octocode_engine::stamp::Proof> {
        if self.untracked {
            return None;
        }
        let mut below = root;
        while !below.join(".git").exists()
            && let Some(above) = below.parent()
        {
            self.watched.push(above.to_path_buf());
            below = above;
        }
        let mut rules = octocode_engine::search::walk::external_ignore_inputs(root);
        rules.extend(self.rules);
        Watch {
            root,
            sources: &self.watched,
            rules: &rules,
            markers: &repository_markers(root),
        }
        .capture(Link::Follow, Some(started))
    }
}

/// Walk `root` for the build files of the anchor's language, group them by the
/// project each roots, and keep the projects other than `server_root`'s that
/// depend on the anchor's, with what the read depended on (`None` when it
/// read nothing but the anchor's directory). `None` when the walk was stopped.
fn read_dependents(
    root: &Path,
    server_root: &Path,
    anchor: &Path,
    extension: &str,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Option<(Dependents, Option<Read>)> {
    let Some((identity, own)) = Identity::of(anchor, extension) else {
        return Some((Dependents::default(), None));
    };
    let names = identity.build_files();
    let mut read = Read::default();
    read.watched.push(own);
    let mut by_root = BTreeMap::<PathBuf, Vec<PathBuf>>::new();
    // The one walk policy of every local tool: dependency and build output
    // and credential stores are never entered.
    for entry in crate::tools::pruned_walk(root, std::iter::empty())
        .build()
        .flatten()
    {
        if stopped() {
            return None;
        }
        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
            read.enter(entry.path(), root);
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if !names.contains(&name.as_ref()) || !entry.file_type().is_some_and(|kind| kind.is_file())
        {
            continue;
        }
        let Some(dir) = entry.path().parent() else {
            continue;
        };
        let probe = dir.join(format!("probe.{extension}"));
        let own = octocode_engine::lsp::workspace::resolve_workspace_root_for_file(
            probe.to_string_lossy().into_owned(),
        )
        .map_or_else(|_| dir.to_path_buf(), |own| canonical_path(&own));
        read.watched.push(entry.path().to_path_buf());
        if own != server_root {
            by_root.entry(own).or_default().push(entry.into_path());
        }
    }
    let roots = by_root
        .into_iter()
        .filter(|(_, files)| files.iter().any(|file| depends_on(file, &identity)))
        .map(|(root, _)| root)
        .collect();
    Some((Dependents { roots: Some(roots) }, Some(read)))
}

/// Test-only count of the dependents walks of each scope root.
#[cfg(test)]
pub(in super::super) mod reads {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{LazyLock, Mutex, PoisonError};

    static WALKS: LazyLock<Mutex<HashMap<PathBuf, usize>>> = LazyLock::new(Mutex::default);

    pub(super) fn note(root: &Path) {
        *WALKS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(root.to_path_buf())
            .or_default() += 1;
    }

    pub(super) fn of(root: &Path) -> usize {
        WALKS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(root)
            .copied()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests;
