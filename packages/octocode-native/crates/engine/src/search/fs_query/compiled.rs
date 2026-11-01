//! A query's options compiled once: filters, limits, the walk's shape, and
//! the verdict on each directory entry before it is matched.

use std::fs;
use std::path::{Path, PathBuf};

use ignore::gitignore::Gitignore;

use super::filters::NameFilter;
use super::{EntryFilter, Visit};
use crate::error::{Error, ErrorKind};
use crate::search::glob;
use crate::search::walk::{IgnoreRules, Prune, RootedPrune};
use crate::types::IgnorePolicy;
use crate::types::{EntryType, FileSystemQueryOptions};

pub(super) struct CompiledQuery {
    pub(super) root: PathBuf,
    pub(super) include_root: bool,
    pub(super) recursive: bool,
    pub(super) stop_at_limit: bool,
    pub(super) max_depth: Option<u32>,
    pub(super) min_depth: u32,
    pub(super) show_hidden: bool,
    /// The ignore rules the walk honors; `None` with `includeIgnored`.
    pub(super) ignore_rules: Option<IgnoreRules>,
    /// The `names`, `extensions` and `regex` filters.
    pub(super) name_filter: NameFilter,
    pub(super) entry_type: Option<EntryType>,
    pub(super) empty: bool,
    pub(super) modified_from: Option<std::time::SystemTime>,
    pub(super) modified_to: Option<std::time::SystemTime>,
    pub(super) accessed_from: Option<std::time::SystemTime>,
    pub(super) accessed_to: Option<std::time::SystemTime>,
    pub(super) size_min: Option<u64>,
    pub(super) size_max: Option<u64>,
    pub(super) permissions: Option<String>,
    pub(super) executable: bool,
    pub(super) readable: bool,
    pub(super) writable: bool,
    pub(super) prune: Prune,
    /// The default excludes, pruned only where git ignores them.
    pub(super) defaults: Option<RootedPrune>,
    /// Caller exclusions (see [`glob`]): a matching directory is skipped with
    /// everything under it.
    pub(super) exclude: Option<Gitignore>,
    pub(super) limit: usize,
    /// How long the walk may inspect new entries (see `skip_entries`).
    pub(super) time_budget: Option<std::time::Duration>,
    /// Leading entries (walk order) an earlier walk inspected: replayed.
    pub(super) skip_entries: u32,
    /// Which walk a [`WalkCursor`](crate::types::fs_resume::WalkCursor) belongs to: everything here that decides
    /// the order and the directories of the walk.
    pub(super) shape: u64,
    /// Keep a window checkpoint at this listed entry.
    pub(super) window_rows: Option<u32>,
    /// The caller's listing rule ([`FileSystemQueryOptions::lists`]).
    pub(super) lists: Option<crate::types::ListsEntry>,
}

impl CompiledQuery {
    pub(super) fn new(options: FileSystemQueryOptions) -> Result<Self, Error> {
        ordered(options.size_min, options.size_max, "sizeBytes")?;
        ordered(options.modified_from, options.modified_to, "modifiedAt")?;
        ordered(options.accessed_from, options.accessed_to, "accessedAt")?;
        let shape = walk_shape(&options);
        let root = PathBuf::from(options.path);
        let defaults = options
            .default_exclude_dir
            .as_ref()
            .map(|names| Prune::new(names).at(&root));
        let name_filter = NameFilter::new(
            &root,
            options.names.as_deref().unwrap_or_default(),
            options.extensions.as_deref().unwrap_or_default(),
            options.regex.as_deref(),
        )?;
        let exclude = options.exclude.unwrap_or_default();
        let exclude = glob::set(
            &root,
            exclude
                .iter()
                .map(|glob| glob.strip_prefix('!').unwrap_or(glob)),
            "exclude",
        )?;

        let min_depth = options.min_depth.unwrap_or(0);
        if options
            .max_depth
            .is_some_and(|max_depth| min_depth > max_depth)
        {
            return Err(Error::with_kind(
                ErrorKind::InvalidInput,
                "minDepth must be less than or equal to maxDepth",
            ));
        }

        let IgnorePolicy {
            include_ignored,
            include_hidden,
        } = options.ignore;
        Ok(Self {
            ignore_rules: (!include_ignored).then(|| IgnoreRules::new(&root)),
            root,
            include_root: options.include_root.unwrap_or(false),
            recursive: options.recursive.unwrap_or(true),
            stop_at_limit: options.stop_at_limit.unwrap_or(true),
            max_depth: options.max_depth,
            min_depth,
            show_hidden: include_hidden,
            name_filter,
            entry_type: options.entry_type,
            empty: options.empty.unwrap_or(false),
            modified_from: options.modified_from,
            modified_to: options.modified_to,
            accessed_from: options.accessed_from,
            accessed_to: options.accessed_to,
            size_min: options.size_min,
            size_max: options.size_max,
            permissions: options.permissions,
            executable: options.executable.unwrap_or(false),
            readable: options.readable.unwrap_or(false),
            writable: options.writable.unwrap_or(false),
            prune: Prune::new(options.exclude_dir.iter().flatten()),
            defaults,
            exclude,
            limit: options.limit.map_or(usize::MAX, |n| n as usize),
            time_budget: options.time_budget,
            skip_entries: options.skip_entries.unwrap_or(0),
            shape,
            window_rows: options.window_rows,
            lists: options.lists,
        })
    }
}

/// A digest of what decides the order and the directories of a walk (never
/// which entries it lists): a [`WalkCursor`](crate::types::fs_resume::WalkCursor) continues only a walk of the
/// same shape.
fn walk_shape(options: &FileSystemQueryOptions) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (
        &options.path,
        options.include_root,
        options.recursive,
        options.max_depth,
        options.ignore.include_ignored,
        options.ignore.include_hidden,
        &options.exclude,
        &options.exclude_dir,
    )
        .hash(&mut hasher);
    hasher.finish()
}

impl CompiledQuery {
    /// Whether a caller `exclude` glob matches this entry.
    fn excludes(&self, path: &Path, is_dir: bool) -> bool {
        self.exclude
            .as_ref()
            .is_some_and(|exclude| glob::matches(exclude, path, is_dir))
    }

    /// What the walk does with a directory entry before it is matched:
    /// the ignore rules, caller exclusions, the policy callback, dot entries
    /// and pruned names decide, in that order. An entry the caller excluded
    /// never reaches the callback, so nothing it counts (withheld, hidden or
    /// pruned entries) includes one. One verdict for a live and a replayed
    /// entry, so a resumed walk recurses where the first did.
    pub(super) fn verdict(
        &self,
        path: &Path,
        file_name: &std::ffi::OsStr,
        is_dir: bool,
        (file_type, visit): (Option<fs::FileType>, Visit),
        admit: &EntryFilter<'_>,
    ) -> Verdict {
        let name = file_name.to_string_lossy();
        if self
            .ignore_rules
            .as_ref()
            .is_some_and(|rules| rules.is_ignored(path, is_dir))
        {
            // A dot entry out of view is not counted as ignored.
            return Verdict::Ignored {
                counted: self.show_hidden || !name.starts_with('.'),
            };
        }
        // A caller exclusion is a path match: it reads no metadata.
        if self.excludes(path, is_dir) {
            return Verdict::Left;
        }
        // Authorize before metadata, matching (which can inspect empty
        // directories), discovery counters, or recursive traversal.
        if !admit(path, file_type, visit) {
            return Verdict::Left;
        }
        if !self.show_hidden && name.starts_with('.') {
            return Verdict::Left;
        }
        if is_dir
            && (self.prune.names(file_name)
                || self
                    .defaults
                    .as_ref()
                    .is_some_and(|defaults| defaults.prunes(path, true)))
        {
            return Verdict::Pruned;
        }
        Verdict::Enters
    }
}

/// [`CompiledQuery::verdict`]: whether an entry is matched and entered, or
/// left out (and how a live walk counts it).
pub(super) enum Verdict {
    Ignored { counted: bool },
    Pruned,
    Left,
    Enters,
}

fn ordered<T: PartialOrd>(from: Option<T>, to: Option<T>, field: &str) -> Result<(), Error> {
    if let (Some(from), Some(to)) = (from, to)
        && from > to
    {
        return Err(Error::with_kind(
            ErrorKind::InvalidInput,
            format!("{field} lower bound must not exceed its upper bound"),
        ));
    }
    Ok(())
}
