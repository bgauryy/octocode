use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};

use super::PathPolicy;
use crate::policy::PolicyErrorCode;
use crate::policy::discovery::{EntryKind, is_sensitive_path};

impl PathPolicy {
    /// Discovery prunes denied subtrees. Let the walker account for ordinary
    /// filesystem errors itself so permission/not-found diagnostics stay intact.
    pub(super) fn permits_discovery(&self, input: impl AsRef<Path>) -> bool {
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
    pub(crate) fn discovery_walk(&self) -> DiscoveryWalk<'_> {
        DiscoveryWalk {
            policy: std::borrow::Cow::Borrowed(self),
            parents: std::sync::RwLock::default(),
        }
    }
}

/// [`PathPolicy::permits_discovery`] over one walk. An entry that is not a
/// symlink resolves to its parent directory's real path plus its own name.
/// The real paths are seeded top-down: the first directory a walk meets
/// (its root) is canonicalized once, and every admitted directory that is
/// not a symlink records its own real path (its parent's plus its name) for
/// its entries, so a walk that follows no link canonicalizes only once; an
/// entry under a parent no admitted directory seeded (a direct call, a
/// path reached another way) canonicalizes that parent once. The decision
/// then applies the same root and sensitive-path checks to the same lexical
/// and real paths; under a directory this walk admitted, that directory's own
/// checks already hold for its entries, so only the entry's name is checked.
pub(crate) struct DiscoveryWalk<'a> {
    policy: std::borrow::Cow<'a, PathPolicy>,
    /// Each walked parent's real path. Read-mostly and shared by a parallel
    /// walk's threads: a known parent takes only the read lock, and a new
    /// one is canonicalized outside any lock.
    parents: std::sync::RwLock<Parents>,
}

/// Each directory by the bytes of its spelling: only the spelling a decision
/// was taken on finds it. Another spelling of that directory misses and takes
/// the whole-path decision, as an unseen directory does.
type Parents =
    std::collections::HashMap<Box<[u8]>, Parent, std::hash::BuildHasherDefault<PathBytesHasher>>;

fn parent_key(directory: &Path) -> &[u8] {
    directory.as_os_str().as_encoded_bytes()
}

/// A multiply-rotate hash of whole words, for the one lookup a walk makes per
/// entry. A collision changes no decision: the map compares the whole key.
#[derive(Default)]
struct PathBytesHasher(u64);

impl PathBytesHasher {
    fn add(&mut self, word: u64) {
        self.0 = self
            .0
            .wrapping_add(word)
            .wrapping_mul(0xf135_7aea_2e62_a9c5);
    }
}

impl std::hash::Hasher for PathBytesHasher {
    fn write(&mut self, bytes: &[u8]) {
        let (words, rest) = bytes.as_chunks::<8>();
        for word in words {
            self.add(u64::from_le_bytes(*word));
        }
        if !rest.is_empty() {
            let mut tail = [0; 8];
            tail[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(tail));
        }
    }

    fn finish(&self) -> u64 {
        self.0.rotate_left(26)
    }
}

/// A directory entries were decided under.
#[derive(Clone, Debug)]
struct Parent {
    real: Real,
    /// This walk admitted it: inside the roots, and no part of its lexical
    /// or real path is withheld.
    admitted: bool,
}

/// A directory's real path, as [`DiscoveryWalk`] keeps it.
#[derive(Clone, Debug)]
enum Real {
    /// The lexical path is real: no symlink on the way to it.
    Same,
    /// Resolved elsewhere (a symlinked ancestor, a `/var` → `/private/var`
    /// root).
    At(PathBuf),
    /// It could not be resolved.
    Unknown,
}

impl Real {
    fn of(lexical: &Path, real: Option<PathBuf>) -> Self {
        match real {
            Some(real) if real == lexical => Self::Same,
            Some(real) => Self::At(real),
            None => Self::Unknown,
        }
    }

    /// The real path of the entry `name` under a directory with this real
    /// path, `None` when it could not be resolved; borrowed when it is the
    /// entry's lexical path.
    fn child<'p>(&self, lexical: &'p Path, name: &std::ffi::OsStr) -> Option<Cow<'p, Path>> {
        match self {
            Self::Same => Some(Cow::Borrowed(lexical)),
            Self::At(real) => Some(Cow::Owned(real.join(name))),
            Self::Unknown => None,
        }
    }
}

impl DiscoveryWalk<'static> {
    /// [`PathPolicy::discovery_walk`] that owns its policy, for a walk
    /// filter the walk shares across threads (`Arc<dyn …>`).
    pub(crate) fn owned(policy: PathPolicy) -> Self {
        Self {
            policy: std::borrow::Cow::Owned(policy),
            parents: std::sync::RwLock::default(),
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Parents this thread's discovery walks canonicalized.
    pub(super) static CANONICALIZED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// A walked entry's path is already absolute and lexical (no `.`, `..` or
/// `~`): [`PathPolicy::expand_and_resolve`] would return it unchanged, so it
/// is used as it is, without a copy.
fn already_resolved(path: &Path) -> bool {
    path.is_absolute()
        && path.components().all(|component| {
            matches!(
                component,
                Component::Normal(_) | Component::RootDir | Component::Prefix(_)
            )
        })
}

impl DiscoveryWalk<'_> {
    pub(crate) fn permits(&self, path: &Path) -> bool {
        self.permits_entry(path, None)
    }

    /// [`Self::permits`] for a walked entry whose directory-listing type the
    /// walk already has: a type that is not a symlink skips the `lstat`.
    pub(crate) fn permits_entry(&self, path: &Path, file_type: Option<std::fs::FileType>) -> bool {
        self.decide(path, file_type).map_or_else(
            || self.policy.permits_discovery(path),
            |(permitted, _)| permitted,
        )
    }

    /// The cached decision on one entry and its listing type, or `None`
    /// when only the full policy can decide it (a symlink, a parent that
    /// does not resolve, a path with no parent).
    fn decide(
        &self,
        path: &Path,
        file_type: Option<std::fs::FileType>,
    ) -> Option<(bool, std::fs::FileType)> {
        if let Some(decided) = self.under_admitted(path, file_type) {
            return Some(decided);
        }
        let lexical: Cow<'_, Path> = if already_resolved(path) {
            Cow::Borrowed(path)
        } else {
            Cow::Owned(self.policy.expand_and_resolve(path))
        };
        let lexical = lexical.as_ref();
        let (parent, name) = (lexical.parent()?, lexical.file_name()?);
        let kind = match file_type {
            Some(file_type) if lexical == path => Some(file_type),
            _ => std::fs::symlink_metadata(lexical)
                .ok()
                .map(|meta| meta.file_type()),
        };
        let kind = kind.filter(|kind| !kind.is_symlink())?;
        // A known parent (every entry after its first sibling, and every
        // entry of a seeded directory) allocates no key.
        let known = self
            .parents
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(parent_key(parent))
            .cloned();
        let parent_real = known.unwrap_or_else(|| {
            #[cfg(test)]
            CANONICALIZED.with(|count| count.set(count.get() + 1));
            let resolved = Parent {
                real: Real::of(parent, std::fs::canonicalize(parent).ok()),
                admitted: false,
            };
            self.parents
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(parent_key(parent).into(), resolved.clone());
            resolved
        });
        let parent_real = parent_real.real;
        let real = parent_real.child(lexical, name)?;
        // Under a parent with no symlink the real path is the lexical one:
        // check it once.
        let moved = !matches!(parent_real, Real::Same);
        // The listing type is the entry's own (never a symlink here), so it
        // is the real path's kind too: no stat.
        let entry = if kind.is_dir() {
            EntryKind::Dir
        } else {
            EntryKind::File
        };
        let permitted = self.policy.allowed(&real)
            && !is_sensitive_path(lexical, entry)
            && (!moved || !is_sensitive_path(&real, entry));
        // An admitted directory that is no link seeds its entries' real
        // path: its parent's plus its own name.
        if permitted && kind.is_dir() {
            self.admit_directory(
                lexical,
                match &real {
                    Cow::Borrowed(_) => Real::Same,
                    Cow::Owned(real) => Real::At(real.clone()),
                },
            );
        }
        Some((permitted, kind))
    }

    /// The decision on an entry of a directory this walk admitted, by its
    /// listing type: no link, so its real path is the directory's plus its
    /// name, inside the roots with it, and nothing above it is withheld.
    /// `None` when the whole path must decide.
    fn under_admitted(
        &self,
        path: &Path,
        file_type: Option<std::fs::FileType>,
    ) -> Option<(bool, std::fs::FileType)> {
        let kind = file_type.filter(|kind| !kind.is_symlink())?;
        let (parent, name) = (path.parent()?, path.file_name()?);
        let real = self
            .parents
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(parent_key(parent))
            .filter(|parent| parent.admitted)?
            .real
            .clone();
        let entry = if kind.is_dir() {
            EntryKind::Dir
        } else {
            EntryKind::File
        };
        let permitted = !crate::policy::discovery::is_sensitive_under_cleared(name, entry)?;
        if permitted && kind.is_dir() {
            let child = match real.child(path, name)? {
                Cow::Borrowed(_) => Real::Same,
                Cow::Owned(real) => Real::At(real),
            };
            self.admit_directory(path, child);
        }
        Some((permitted, kind))
    }

    /// Keep `directory` as admitted, with its real path unless one is known.
    /// Keyed as its entries' `parent()` spells it: no trailing separator.
    fn admit_directory(&self, directory: &Path, real: Real) {
        self.parents
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(parent_key(directory.components().as_path()).into())
            .and_modify(|known| known.admitted = true)
            .or_insert(Parent {
                real,
                admitted: true,
            });
    }
}

/// A text search's path filter: a directory the policy admits
/// ([`PathPolicy::validate`]) is entered and a file it admits for reading
/// ([`PathPolicy::validate_read`]: inside the roots, not sensitive, a
/// regular file) is read. A walked entry that is no symlink is decided from
/// its parent's real path ([`DiscoveryWalk`]), so a walk canonicalizes once
/// per resolved parent, not once per file; a symlink, or an entry the walk
/// cannot place, takes the full validation, which resolves the link and
/// denies one that escapes the roots.
impl octocode_engine::search::text_search::TextSearchPathFilter for DiscoveryWalk<'_> {
    fn allows(&self, path: &Path, is_dir: bool) -> bool {
        self.admits(path, None, is_dir)
    }

    fn allows_entry(&self, path: &Path, file_type: Option<std::fs::FileType>) -> bool {
        self.admits(path, file_type, file_type.is_some_and(|kind| kind.is_dir()))
    }
}

impl DiscoveryWalk<'_> {
    /// The read filter's decision on `path`, entered as a directory when
    /// `is_dir`.
    fn admits(&self, path: &Path, file_type: Option<std::fs::FileType>, is_dir: bool) -> bool {
        match self.decide(path, file_type) {
            Some((permitted, kind)) => permitted && (kind.is_dir() || kind.is_file()),
            None if is_dir => self.policy.validate(path).is_ok(),
            None => self.policy.validate_read(path).is_ok(),
        }
    }
}
