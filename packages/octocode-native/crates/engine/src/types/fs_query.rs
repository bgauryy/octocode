//! Filesystem query options, entries and results.

/// A listing rule the caller owns ([`FileSystemQueryOptions::lists`]):
/// whether the entry `path` (a directory when the flag is set) at walk
/// position `u32` (as [`FileSystemQueryResult::walk_position`] counts) is
/// listed. It is asked once per fresh entry, so a resumed window asks only
/// of its own.
#[derive(Clone)]
pub struct ListsEntry(pub std::sync::Arc<ListsRule>);

/// The rule a [`ListsEntry`] holds.
pub type ListsRule = dyn Fn(&std::path::Path, bool, u32) -> bool + Send + Sync;

impl std::fmt::Debug for ListsEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ListsEntry")
    }
}

#[derive(Debug, Clone, Default)]
pub struct FileSystemQueryOptions {
    pub path: String,
    /// Include the root path itself in results (default false).
    pub include_root: Option<bool>,
    /// Descend into child directories (default true).
    pub recursive: Option<bool>,
    /// Maximum depth where direct children are depth 1.
    pub max_depth: Option<u32>,
    /// Minimum depth where direct children are depth 1.
    pub min_depth: Option<u32>,
    /// Which ignore rules and dot entries the walk leaves out.
    pub ignore: crate::types::IgnorePolicy,
    /// List entries one of these globs matches (`search::glob`: a glob
    /// without `/` matches a name at any depth, one with `/` the path below
    /// the root).
    pub names: Option<Vec<String>>,
    /// Match file extensions, OR-combined. Values may include a leading dot.
    /// Directories are preserved so recursive structure views can keep context.
    pub extensions: Option<Vec<String>>,
    /// Rust regex against basename.
    pub regex: Option<String>,
    /// List only files or only directories (other entries are still walked).
    pub entry_type: Option<EntryType>,
    /// Match only empty files or directories.
    pub empty: Option<bool>,
    /// Inclusive modification-time bounds, as absolute instants.
    pub modified_from: Option<std::time::SystemTime>,
    pub modified_to: Option<std::time::SystemTime>,
    /// Inclusive access-time bounds, as absolute instants.
    pub accessed_from: Option<std::time::SystemTime>,
    pub accessed_to: Option<std::time::SystemTime>,
    /// Inclusive file-size bounds, in bytes; directories never match.
    pub size_min: Option<u64>,
    pub size_max: Option<u64>,
    /// Exact octal permissions, e.g. 644.
    pub permissions: Option<String>,
    pub executable: Option<bool>,
    pub readable: Option<bool>,
    pub writable: Option<bool>,
    /// Directory names pruned from recursive traversal. The engine has no
    /// default list: omission prunes nothing. The native runtime's prune
    /// policy (`policy::prune`) supplies the names for every tool walk.
    /// Pruned directories are reported in `pruned_dirs`.
    pub exclude_dir: Option<Vec<String>>,
    /// Default-excluded directory names (see [`Prune::at`]): pruned outside a
    /// git work tree, and inside one only where git ignores them. Reported in
    /// `pruned_dirs` too.
    ///
    /// [`Prune::at`]: crate::search::walk::Prune::at
    pub default_exclude_dir: Option<Vec<String>>,
    /// The caller's globs to skip (`search::glob`). A matching directory is
    /// skipped with everything under it.
    pub exclude: Option<Vec<String>>,
    /// Stop walking after `limit` returned entries. Default true for interactive
    /// tools; set false when exact total_discovered is more important than
    /// latency.
    pub stop_at_limit: Option<bool>,
    /// Store at most this many matching entries while still counting matches
    /// when stop_at_limit is false. `None` stores every one.
    pub limit: Option<u32>,
    /// Wall time the walk may spend before it stops at the next entry and
    /// reports where in [`FileSystemQueryResult::time_cut`]. `None` walks
    /// to the end.
    pub time_budget: Option<std::time::Duration>,
    /// Resume position: the leading entries (walk order, as
    /// [`FileSystemQueryResult::walk_position`] counts them) an earlier walk
    /// of the same query already inspected. The walk replays them without
    /// listing or counting them (it still reads the directories it must
    /// enter to find where they end, and judges only those), and
    /// `time_budget` starts after them, so a resumed walk always inspects
    /// new entries.
    pub skip_entries: Option<u32>,
    /// Where the walk `skip_entries` resumes stood. When it was made by a
    /// walk of the same shape at exactly that position, the resumed walk
    /// continues from it and replays nothing; otherwise it is ignored and
    /// the walk replays as above.
    pub resume: Option<crate::types::fs_resume::WalkCursor>,
    /// The caller's last listing rule, asked of an entry every other filter
    /// lists: whether it is listed. It never prunes: the walk still enters
    /// every directory it would enter. `None` lists all.
    pub lists: Option<ListsEntry>,
    /// Keep a [`FileSystemQueryResult::window_cursor`] at the listed entry
    /// this many entries into the walk (the root entry is none), for a
    /// window that ends there ([`FileSystemQueryResult::end_at`]).
    pub window_rows: Option<u32>,
}

/// The entries a filesystem query lists ([`FileSystemQueryOptions::entry_type`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    File,
    Directory,
}

/// What a listed entry is, from its `symlink_metadata` (a link is never
/// followed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Clone)]
pub struct FileSystemEntry {
    /// Absolute or input-root-relative path as returned by the platform.
    pub path: String,
    /// Path relative to the query root.
    pub relative_path: String,
    pub name: String,
    pub entry_type: FileKind,
    pub size: Option<i64>,
    /// The exact modification time, for change checks.
    pub modified_time: Option<std::time::SystemTime>,
    /// The entry's position in the walk (1-based, [`FileSystemQueryResult::walk_position`]
    /// counts the same way): a walk resumed at it (`skip_entries`) starts
    /// at the entry after it. 0 for the root.
    pub walk_position: u32,
}

impl FileSystemEntry {
    /// [`Self::modified_time`] in milliseconds since the Unix epoch.
    pub fn modified_ms(&self) -> Option<f64> {
        self.modified_time
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|elapsed| elapsed.as_millis() as f64)
    }
}

#[derive(Debug, Clone, Default)]
pub struct FileSystemQueryResult {
    pub entries: Vec<FileSystemEntry>,
    pub total_discovered: u32,
    pub was_capped: bool,
    /// The first match the `limit` left unlisted (absolute path), in walk
    /// order: every entry the walk did not inspect sorts after it, and it is
    /// the first match the listing lacks. `None` when `limit` cut nothing.
    pub first_unlisted: Option<String>,
    pub skipped: u32,
    pub permission_denied: u32,
    /// Root-relative paths of the entries the ignore rules left out (the
    /// topmost of each ignored subtree), in walk order, a directory's with a
    /// trailing `/` (a caller's own prune may name some of them too); a dot
    /// entry out of view is not counted.
    pub ignored: Vec<String>,
    /// Root-relative paths of the directories `exclude_dir` pruned, in walk
    /// order: what a listing or an empty result did not cover.
    pub pruned_dirs: Vec<String>,
    /// Root-relative path of the first entry the walk did not inspect
    /// because its `time_budget` ran out. The walk is name-ordered and
    /// depth-first, so every entry before it in that order was inspected and
    /// every entry from it on was not. `None`: the walk was not cut by time.
    pub time_cut: Option<String>,
    /// Entries the walk inspected, in walk order, `skip_entries` included:
    /// the `skip_entries` that resumes at the first uninspected entry.
    pub walk_position: u32,
    /// What the walk counted, by position: [`Self::end_at`] recounts from it.
    pub tally: crate::types::fs_resume::WalkTally,
    /// Where a walk cut by time stood: its `time_cut` entry is the next one
    /// a resume inspects. `None` when the walk was not cut, or after
    /// [`Self::end_at`] moved the end elsewhere.
    pub cursor: Option<crate::types::fs_resume::WalkCursor>,
    /// Where the walk stood at the entry `window_rows` named.
    pub window_cursor: Option<crate::types::fs_resume::WalkCursor>,
}

impl FileSystemQueryResult {
    /// End the window at `position`: only the entries through it, and only
    /// what the walk counted through it, so a walk resumed at `position`
    /// (`skip_entries`) counts the rest once. The cursor is the one kept at
    /// that position, if any.
    pub fn end_at(&mut self, position: u32) {
        self.entries.retain(|entry| entry.walk_position <= position);
        // A walk cut by time stored every match it found (it stopped before
        // its entry limit), so the entries are the matches.
        self.total_discovered = u32::try_from(self.entries.len()).unwrap_or(u32::MAX);
        self.was_capped = false;
        let totals = self.tally.through(position);
        self.skipped = totals.skipped;
        self.permission_denied = totals.permission_denied;
        self.ignored = totals.ignored;
        self.pruned_dirs = totals.pruned_dirs;
        self.walk_position = position;
        let window = self.window_cursor.take();
        let end = self.cursor.take();
        self.cursor = [window, end]
            .into_iter()
            .flatten()
            .find(|cursor| cursor.position() == position);
    }
}
