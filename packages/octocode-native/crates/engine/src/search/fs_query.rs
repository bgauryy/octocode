use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use ignore::gitignore::Gitignore;
use regex::Regex;

use std::sync::Arc;

use super::fs_resume::{Child, Frame, WalkCursor, WalkTally};
#[path = "fs_query_read_ahead.rs"]
mod read_ahead;
use super::glob;
use super::walk::{IgnorePolicy, IgnoreRules, Prune, Stop};
use crate::error::{Error, ErrorKind};
use crate::text::file_extension::extension_of;
use crate::types::{
    EntryType, FileKind, FileSystemEntry, FileSystemQueryOptions, FileSystemQueryResult,
};
use read_ahead::ReadAhead;

/// Whether the walk inspects an entry now or replays one an earlier window
/// of a resumed walk (`skip_entries`) already inspected. A policy callback
/// that counts what it leaves out counts only a [`Visit::Fresh`] entry: the
/// earlier window counted a replayed one. A fresh entry names its walk
/// position, so a window that ends at an earlier entry can drop the counts of
/// the ones after it (the window that resumes there counts them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visit {
    Fresh { position: u32 },
    Replayed,
}

impl Visit {
    /// The walk position of a fresh entry; `None` for a replayed one.
    #[must_use]
    pub fn fresh_at(self) -> Option<u32> {
        match self {
            Self::Fresh { position } => Some(position),
            Self::Replayed => None,
        }
    }
}

/// A walk's policy callback: the path, its directory-entry type (`None`
/// for the root, or when the platform did not report it) and whether the
/// entry is [`Visit::Fresh`]. `false` leaves the entry out, and a directory
/// with everything under it.
pub type EntryFilter<'a> = dyn Fn(&Path, Option<fs::FileType>, Visit) -> bool + 'a;

const DEFAULT_LIMIT: usize = 10_000;
/// Hard ceiling on directory-recursion depth. Symlink cycles are already
/// avoided (symlink_metadata never reports a symlink as a dir), so this only
/// guards against pathologically deep real trees overflowing the stack when no
/// `max_depth` was supplied. Far deeper than any realistic project layout.
const MAX_RECURSION_DEPTH: u32 = 100;

struct CompiledQuery {
    root: PathBuf,
    include_root: bool,
    recursive: bool,
    stop_at_limit: bool,
    max_depth: Option<u32>,
    min_depth: u32,
    show_hidden: bool,
    /// The ignore rules the walk honors; `None` with `includeIgnored`.
    ignore_rules: Option<IgnoreRules>,
    /// The `names`, `extensions` and `regex` filters.
    name_filter: NameFilter,
    entry_type: Option<EntryType>,
    empty: bool,
    modified_within_secs: Option<u64>,
    modified_before_secs: Option<u64>,
    accessed_within_secs: Option<u64>,
    size_greater: Option<u64>,
    size_less: Option<u64>,
    permissions: Option<String>,
    executable: bool,
    readable: bool,
    writable: bool,
    prune: Prune,
    /// The default excludes, pruned only where git ignores them.
    defaults: Option<super::walk::RootedPrune>,
    /// Caller exclusions (see [`glob`]): a matching directory is skipped with
    /// everything under it.
    exclude: Option<Gitignore>,
    limit: usize,
    /// How long the walk may inspect new entries (see `skip_entries`).
    time_budget: Option<std::time::Duration>,
    /// Leading entries (walk order) an earlier walk inspected: replayed.
    skip_entries: u32,
    /// Which walk a [`WalkCursor`] belongs to: everything here that decides
    /// the order and the directories of the walk.
    shape: u64,
    /// Keep a window checkpoint at this listed entry.
    window_rows: Option<u32>,
}

#[derive(Default)]
struct QueryState {
    /// When the walk stops at its next entry; set when the first new entry
    /// is reached, so replayed entries spend none of the budget.
    deadline: Option<Instant>,
    /// Entries inspected so far, replayed ones included.
    position: u32,
    entries: Vec<FileSystemEntry>,
    /// Entries listed (stored, the root excepted) so far.
    listed: u32,
    total_discovered: u32,
    /// The first match the entry limit left unlisted.
    first_unlisted: Option<PathBuf>,
    tally: WalkTally,
    time_cut: Option<String>,
    /// The directories the walk is inside, outermost first.
    stack: Vec<Frame>,
    /// Where the walk stood when it was cut by time.
    cursor: Option<WalkCursor>,
    /// Where the walk stood at the `window_rows`-th listed entry.
    window_cursor: Option<WalkCursor>,
    /// Directories the walk read, itself or ahead.
    directories_read: u32,
    /// The workers reading the walk's next directories, once it read
    /// [`read_ahead::START_AFTER_DIRECTORIES`] itself.
    ahead: Option<ReadAhead>,
}

impl QueryState {
    /// Count an entry or directory that could not be read.
    fn unreadable(&mut self, err: &std::io::Error) {
        let denied = err.kind() == std::io::ErrorKind::PermissionDenied;
        self.tally.unreadable(self.position, denied);
    }

    /// Where the walk stands: the directories it is inside and, when the
    /// walk is about to enter one, that directory.
    fn cursor(&self, shape: u64, entering: Option<Frame>) -> WalkCursor {
        let mut frames = self.stack.clone();
        frames.extend(entering);
        WalkCursor {
            position: self.position,
            shape,
            frames,
        }
    }

    /// The walk has nothing more to inspect: out of time or past the limit.
    fn finished(&self, query: &CompiledQuery) -> bool {
        self.time_cut.is_some()
            || (query.stop_at_limit && self.total_discovered as usize > query.limit)
    }
}

/// Walk `options.path` in name order, listing the entries its filters
/// match. `admit` decides each descendant before it is inspected or counted
/// (a denied directory is not entered); ignore rules and caller exclusions
/// decide it before that.
/// `stop` cancels the walk ([`ErrorKind::Cancelled`]).
pub fn query_file_system(
    options: FileSystemQueryOptions,
    admit: &EntryFilter<'_>,
    stop: &Stop<'_>,
) -> Result<FileSystemQueryResult, Error> {
    let resume = options.resume.clone();
    let query = CompiledQuery::new(options)?;
    let mut state = QueryState::default();
    // A resumed walk's root was listed (and counted) by the walk it resumes.
    let root_visit = if query.skip_entries == 0 {
        Visit::Fresh { position: 0 }
    } else {
        Visit::Replayed
    };
    if !admit(&query.root, None, root_visit) {
        return Err(Error::new("Filesystem query root is denied by path policy"));
    }
    let root_metadata = fs::symlink_metadata(&query.root).map_err(|err| {
        Error::io(
            &err,
            format!(
                "Cannot access filesystem query root '{}': {err}",
                query.root.display()
            ),
        )
    })?;

    if query.include_root && query.skip_entries == 0 {
        let mut metadata = Some(root_metadata.clone());
        let kind = EntryKind::of(root_metadata.file_type());
        let name = file_name(&query.root);
        // The root's metadata is in hand, so the visit cannot fail to stat it.
        let _ = visit_path(
            &query.root,
            &name,
            0,
            kind,
            &mut metadata,
            &query,
            &mut state,
        );
    }

    if root_metadata.is_dir() && (query.recursive || !query.include_root) {
        // A cursor made by this walk's shape at the resume position continues
        // it without replaying a single entry.
        let frames = resume
            .filter(|cursor| cursor.shape == query.shape && cursor.position == query.skip_entries)
            .map(|cursor| cursor.frames);
        if let Some((first, inner)) = frames.as_deref().and_then(<[Frame]>::split_first) {
            state.position = query.skip_entries;
            walk_frame(first.clone(), inner, &query, &mut state, admit, stop)?;
        } else {
            walk_children(&query.root, 1, &query, &mut state, admit, stop)?;
        }
    } else if !root_metadata.is_dir() && !query.include_root {
        return Err(Error::new(format!(
            "Filesystem query root is not a directory: {}",
            query.root.display()
        )));
    }

    let was_capped = state.total_discovered as usize > query.limit;
    let totals = state.tally.through(u32::MAX);
    Ok(FileSystemQueryResult {
        entries: state.entries,
        total_discovered: state.total_discovered,
        was_capped,
        first_unlisted: state
            .first_unlisted
            .map(|path| path.to_string_lossy().into_owned()),
        skipped: totals.skipped,
        permission_denied: totals.permission_denied,
        ignored: totals.ignored,
        pruned_dirs: totals.pruned_dirs,
        time_cut: state.time_cut,
        walk_position: state.position,
        tally: state.tally,
        cursor: state.cursor,
        window_cursor: state.window_cursor,
    })
}

impl CompiledQuery {
    fn new(options: FileSystemQueryOptions) -> Result<Self, Error> {
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
            modified_within_secs: parse_duration_option(
                options.modified_within.as_deref(),
                "modifiedWithin",
            )?,
            modified_before_secs: parse_duration_option(
                options.modified_before.as_deref(),
                "modifiedBefore",
            )?,
            accessed_within_secs: parse_duration_option(
                options.accessed_within.as_deref(),
                "accessedWithin",
            )?,
            size_greater: parse_size_option(options.size_greater.as_deref(), "sizeGreater")?,
            size_less: parse_size_option(options.size_less.as_deref(), "sizeLess")?,
            permissions: options.permissions,
            executable: options.executable.unwrap_or(false),
            readable: options.readable.unwrap_or(false),
            writable: options.writable.unwrap_or(false),
            prune: Prune::new(options.exclude_dir.iter().flatten()),
            defaults,
            exclude,
            limit: options.limit.map(|n| n as usize).unwrap_or(DEFAULT_LIMIT),
            time_budget: options.time_budget,
            skip_entries: options.skip_entries.unwrap_or(0),
            shape,
            window_rows: options.window_rows,
        })
    }
}

/// A digest of what decides the order and the directories of a walk (never
/// which entries it lists): a [`WalkCursor`] continues only a walk of the
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

/// Whether the `entry_type` filter lists an entry of `kind`.
fn lists(entry_type: EntryType, kind: EntryKind) -> bool {
    match entry_type {
        EntryType::File => kind.is_file,
        EntryType::Directory => kind.is_dir,
    }
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
    fn verdict(
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
enum Verdict {
    Ignored { counted: bool },
    Pruned,
    Left,
    Enters,
}

/// Walk the directory `base` (a fresh one: read now) and everything under it.
fn walk_children(
    base: &Path,
    depth: u32,
    query: &CompiledQuery,
    state: &mut QueryState,
    admit: &EntryFilter<'_>,
    stop: &Stop<'_>,
) -> Result<(), Error> {
    let frame = Frame {
        base: base.to_path_buf(),
        depth,
        children: None,
        read_at: None,
        next: 0,
    };
    walk_frame(frame, &[], query, state, admit, stop)
}

/// A directory's entries in walk order, or `None` when it is too deep, past
/// `max_depth`, or unreadable (counted, unless the walk this one resumes
/// counted it).
fn read_children(
    base: &Path,
    depth: u32,
    query: &CompiledQuery,
    state: &mut QueryState,
) -> Option<(Vec<Child>, Option<std::time::SystemTime>)> {
    // The walk this one resumes read this directory when it inspected the
    // entry that led here (the root: before its first entry), and counted
    // what failed to read: counted again, a failure would show twice.
    let reread = query.skip_entries > 0 && state.position <= query.skip_entries;
    if depth > MAX_RECURSION_DEPTH {
        if !reread {
            state.tally.unreadable(state.position, false);
        }
        return None;
    }
    if query.max_depth.is_some_and(|max_depth| depth > max_depth) {
        return None;
    }

    let listing = match state.ahead.as_mut().and_then(|ahead| ahead.take(base)) {
        Some(listing) => listing,
        None => read_ahead::read_listing(base),
    };
    state.directories_read += 1;
    if state.ahead.is_none() && state.directories_read == read_ahead::START_AFTER_DIRECTORIES {
        state.ahead = ReadAhead::start();
    }
    // Read errors are surfaced as skips.
    let (children, errors) = match listing.entries {
        Ok(read) => read,
        Err(err) => {
            if !reread {
                state.unreadable(&err);
            }
            return None;
        }
    };
    if !reread {
        for err in &errors {
            state.unreadable(err);
        }
    }
    Some((children, listing.read_at))
}

/// Walk one directory of the walk: `frame` from its `next` entry on, the
/// directories `inner` (a cursor's deeper ones, outermost first) before it.
fn walk_frame(
    mut frame: Frame,
    inner: &[Frame],
    query: &CompiledQuery,
    state: &mut QueryState,
    admit: &EntryFilter<'_>,
    stop: &Stop<'_>,
) -> Result<(), Error> {
    let children = match frame.children.clone() {
        Some(children) => children,
        None => match read_children(&frame.base, frame.depth, query, state) {
            Some((children, read_at)) => {
                frame.read_at = read_at;
                Arc::new(children)
            }
            None => return Ok(()),
        },
    };
    frame.children = Some(children);
    state.stack.push(frame);
    let level = state.stack.len() - 1;
    let walked = walk_level(level, inner, query, state, admit, stop);
    state.stack.pop();
    walked
}

fn walk_level(
    level: usize,
    inner: &[Frame],
    query: &CompiledQuery,
    state: &mut QueryState,
    admit: &EntryFilter<'_>,
    stop: &Stop<'_>,
) -> Result<(), Error> {
    if let Some((first, rest)) = inner.split_first() {
        walk_frame(first.clone(), rest, query, state, admit, stop)?;
        if state.finished(query) {
            return Ok(());
        }
    }
    let frame = &state.stack[level];
    let (base, depth, start) = (frame.base.clone(), frame.depth, frame.next);
    let children = frame.children.clone().unwrap_or_default();
    // Child directories this one reads at the next depth: the ones read
    // ahead while the walk is inside an earlier sibling.
    let reads_next = query.recursive
        && depth < MAX_RECURSION_DEPTH
        && query.max_depth.is_none_or(|max_depth| depth < max_depth);
    // The directory child last inspected: when the walk did not enter it,
    // its listing read ahead is dropped.
    let mut passed: Option<PathBuf> = None;

    for (index, child) in children.iter().enumerate().skip(start) {
        if let Some(dir) = passed.take()
            && let Some(ahead) = state.ahead.as_mut()
        {
            ahead.drop_unused(&dir);
        }
        // The next entry of this directory a resume inspects.
        state.stack[level].next = index;
        // Look ahead to one additional matching entry before declaring overflow.
        // Reaching the stored-entry cap alone does not prove the scan is partial.
        if query.stop_at_limit && state.total_discovered as usize > query.limit {
            return Ok(());
        }
        if stop() {
            return Err(Error::cancelled());
        }
        let file_name = child.name.as_os_str();
        // The leading `skip_entries` entries were inspected by the walk this
        // one resumes: replayed below, they spend none of the time budget.
        let replaying = state.position < query.skip_entries;
        if !replaying && let Some(budget) = query.time_budget {
            // The budget starts at the first new entry, which a positive
            // budget always inspects: every resumed window makes progress.
            let first = state.deadline.is_none();
            let deadline = *state
                .deadline
                .get_or_insert_with(|| crate::scan_budget::walk_now() + budget);
            if (budget.is_zero() || !first) && crate::scan_budget::walk_now() >= deadline {
                let path = base.join(file_name);
                let relative = path.strip_prefix(&query.root).unwrap_or(&path);
                state.time_cut = Some(crate::text::slash_path(relative));
                state.cursor = Some(state.cursor(query.shape, None));
                return Ok(());
            }
        }
        state.position += 1;
        state.stack[level].next = index + 1;

        let path = base.join(file_name);
        let file_type = child.file_type;
        let mut metadata = None;
        let kind = match file_type {
            Some(file_type) => EntryKind::of(file_type),
            // A replayed entry was counted by the walk it resumes.
            None if replaying => match fs::symlink_metadata(&path) {
                Ok(meta) => meta.file_type().into(),
                Err(_) => continue,
            },
            None => match lstat(&path, state) {
                Some(meta) => metadata.insert(meta).file_type().into(),
                None => continue,
            },
        };
        let is_directory = kind.is_dir;
        if is_directory {
            passed = Some(path.clone());
        }
        // A replayed entry only counts: the walk it resumes listed it or
        // not, and entered it or not by this same verdict, which only a
        // directory's entry needs again.
        if replaying && !(query.recursive && is_directory) {
            continue;
        }
        let visit = if replaying {
            Visit::Replayed
        } else {
            Visit::Fresh {
                position: state.position,
            }
        };
        match query.verdict(&path, file_name, is_directory, (file_type, visit), admit) {
            Verdict::Ignored { counted } => {
                if counted && !replaying {
                    state.tally.ignored(state.position);
                }
                continue;
            }
            Verdict::Pruned => {
                if !replaying {
                    let relative = path.strip_prefix(&query.root).unwrap_or(&path);
                    state
                        .tally
                        .pruned(state.position, crate::text::slash_path(relative));
                }
                continue;
            }
            Verdict::Left => continue,
            Verdict::Enters => {}
        }

        if !replaying {
            let listed = state.entries.len();
            if visit_path(
                &path,
                &file_name.to_string_lossy(),
                depth,
                kind,
                &mut metadata,
                query,
                state,
            )
            .is_err()
            {
                continue;
            }
            if state.entries.len() > listed && state.position > 0 {
                state.listed += 1;
                if query.window_rows == Some(state.listed) {
                    let entering = (query.recursive && is_directory).then(|| Frame {
                        base: path.clone(),
                        depth: depth + 1,
                        children: None,
                        read_at: None,
                        next: 0,
                    });
                    state.window_cursor = Some(state.cursor(query.shape, entering));
                }
            }
        }

        if query.recursive && is_directory {
            if reads_next && let Some(ahead) = state.ahead.as_mut() {
                ahead.queue(
                    children[index + 1..]
                        .iter()
                        .filter(|child| child.file_type.is_some_and(|kind| kind.is_dir()))
                        .map(|child| base.join(&child.name)),
                );
            }
            walk_children(&path, depth + 1, query, state, admit, stop)?;
            if state.finished(query) {
                return Ok(());
            }
        }
    }
    if let Some(dir) = passed
        && let Some(ahead) = state.ahead.as_mut()
    {
        ahead.drop_unused(&dir);
    }
    Ok(())
}

/// An entry's type, from its directory listing or its `symlink_metadata`.
#[derive(Clone, Copy)]
struct EntryKind {
    is_dir: bool,
    is_file: bool,
}

impl EntryKind {
    fn of(file_type: fs::FileType) -> Self {
        Self {
            is_dir: file_type.is_dir(),
            is_file: file_type.is_file(),
        }
    }
}

impl From<fs::FileType> for EntryKind {
    fn from(file_type: fs::FileType) -> Self {
        Self::of(file_type)
    }
}

/// `symlink_metadata`, counting a failure as a skipped entry.
fn lstat(path: &Path, state: &mut QueryState) -> Option<fs::Metadata> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(err) => {
            state.unreadable(&err);
            None
        }
    }
}

/// Count and record `path` when it matches. Name and type filters run
/// first; the entry is stat'ed (into `metadata`) only when they pass. `Err`
/// when that stat failed: the entry is skipped like an unreadable one.
fn visit_path(
    path: &Path,
    name: &str,
    depth: u32,
    kind: EntryKind,
    metadata: &mut Option<fs::Metadata>,
    query: &CompiledQuery,
    state: &mut QueryState,
) -> Result<(), ()> {
    if depth < query.min_depth {
        return Ok(());
    }
    if query.max_depth.is_some_and(|max_depth| depth > max_depth) {
        return Ok(());
    }
    if !matches_names(path, name, kind, query) {
        return Ok(());
    }
    let metadata = match metadata {
        Some(metadata) => metadata,
        None => metadata.insert(lstat(path, state).ok_or(())?),
    };
    if !matches_metadata(path, metadata, query) {
        return Ok(());
    }

    state.total_discovered += 1;
    if state.entries.len() >= query.limit {
        if state.first_unlisted.is_none() {
            state.first_unlisted = Some(path.to_path_buf());
        }
        return Ok(());
    }

    state
        .entries
        .push(to_entry(path, state.position, metadata, query));
    Ok(())
}

/// The filters an entry's name, path and type decide.
fn matches_names(path: &Path, name: &str, kind: EntryKind, query: &CompiledQuery) -> bool {
    query
        .name_filter
        .lists(path, name, kind.is_dir, kind.is_file)
        && query
            .entry_type
            .is_none_or(|entry_type| lists(entry_type, kind))
}

/// A query's name filters, compiled once: `names` globs (see [`glob`]: an
/// entry is listed when one matches it), `extensions` (OR-combined, files
/// only) and a `regex` over the entry's name. An empty filter lists all.
pub struct NameFilter {
    names: Option<Gitignore>,
    extensions: Vec<String>,
    regex: Option<Regex>,
}

impl NameFilter {
    /// The filters of a walk rooted at `root`. A glob or regex that does not
    /// compile fails ([`ErrorKind::InvalidGlob`], [`ErrorKind::InvalidPattern`]):
    /// a filter is never silently dropped.
    pub fn new(
        root: &Path,
        names: &[String],
        extensions: &[String],
        regex: Option<&str>,
    ) -> Result<Self, Error> {
        // An extension never holds a glob or path character: `fs_*` or
        // `src/*.ts` is a name glob, which would match nothing here.
        if let Some(glob) = extensions
            .iter()
            .find(|value| value.contains(['*', '?', '[', '/', '\\']))
        {
            return Err(Error::with_kind(
                ErrorKind::InvalidPattern,
                format!(
                    "extensions take bare extensions such as \"rs\"; \"{glob}\" is a name glob: pass it as include"
                ),
            ));
        }
        Ok(Self {
            names: glob::set(root, names.iter().map(String::as_str), "names")?,
            extensions: normalize_extensions(extensions.to_vec()),
            regex: regex
                .map(|pattern| {
                    Regex::new(pattern).map_err(|err| {
                        Error::with_kind(
                            ErrorKind::InvalidPattern,
                            format!("Invalid regex for local filesystem query: {err}"),
                        )
                    })
                })
                .transpose()?,
        })
    }

    /// Whether the entry at `path` named `name` passes every filter. A
    /// directory never passes an extension filter (the walk still enters
    /// it).
    pub fn lists(&self, path: &Path, name: &str, is_dir: bool, is_file: bool) -> bool {
        self.names
            .as_ref()
            .is_none_or(|names| glob::matches(names, path, is_dir))
            && self.regex.as_ref().is_none_or(|regex| regex.is_match(name))
            && (self.extensions.is_empty() || (is_file && has_extension(name, &self.extensions)))
    }
}

/// The filters that need the entry's metadata.
fn matches_metadata(path: &Path, metadata: &fs::Metadata, query: &CompiledQuery) -> bool {
    if query.empty && !is_empty(path, metadata) {
        return false;
    }
    // A size filter is about content: a directory's own length (its entry
    // table) is none, so a directory never matches one.
    let sized = query.size_greater.is_some() || query.size_less.is_some();
    if sized && metadata.is_dir() {
        return false;
    }
    if let Some(min_size) = query.size_greater
        && metadata.len() <= min_size
    {
        return false;
    }
    if let Some(max_size) = query.size_less
        && metadata.len() >= max_size
    {
        return false;
    }
    if !matches_time_filters(metadata, query) {
        return false;
    }
    if !matches_permissions(metadata, query) {
        return false;
    }

    true
}

/// Whether `name` ends in one of `extensions`, normalized as a query's are
/// (lowercase, no leading dot).
pub(crate) fn has_extension(name: &str, extensions: &[String]) -> bool {
    let extension = extension_of(name, true, "");
    !extension.is_empty() && extensions.iter().any(|allowed| allowed == &extension)
}

fn matches_time_filters(metadata: &fs::Metadata, query: &CompiledQuery) -> bool {
    if let Some(duration) = query.modified_within_secs
        && !system_time_within(metadata.modified().ok(), duration)
    {
        return false;
    }
    if let Some(duration) = query.modified_before_secs
        && !system_time_before(metadata.modified().ok(), duration)
    {
        return false;
    }
    if let Some(duration) = query.accessed_within_secs
        && !system_time_within(metadata.accessed().ok(), duration)
    {
        return false;
    }
    true
}

fn system_time_within(time: Option<SystemTime>, seconds: u64) -> bool {
    time.and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|elapsed| elapsed.as_secs() <= seconds)
}

fn system_time_before(time: Option<SystemTime>, seconds: u64) -> bool {
    time.and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|elapsed| elapsed.as_secs() > seconds)
}

#[cfg(unix)]
fn matches_permissions(metadata: &fs::Metadata, query: &CompiledQuery) -> bool {
    use std::os::unix::fs::PermissionsExt;

    let mode = metadata.permissions().mode() & 0o777;
    if let Some(expected) = query.permissions.as_ref() {
        // Compare octal *values*, not strings. The old `trim_start_matches('0')`
        // turned "000" into "" (so a real 000 file could never match) and also
        // mishandled forms like "0644"/"0o644". Parse both and compare numbers.
        let expected_mode = u32::from_str_radix(expected.trim().trim_start_matches("0o"), 8).ok();
        if expected_mode != Some(mode) {
            return false;
        }
    }
    if query.executable && mode & 0o111 == 0 {
        return false;
    }
    if query.readable && mode & 0o444 == 0 {
        return false;
    }
    if query.writable && mode & 0o222 == 0 {
        return false;
    }
    true
}

#[cfg(not(unix))]
fn matches_permissions(metadata: &fs::Metadata, query: &CompiledQuery) -> bool {
    if query.permissions.is_some() || query.executable || query.readable {
        return true;
    }
    if query.writable {
        return !metadata.permissions().readonly();
    }
    true
}

fn is_empty(path: &Path, metadata: &fs::Metadata) -> bool {
    if metadata.is_file() {
        return metadata.len() == 0;
    }
    if metadata.is_dir() {
        return fs::read_dir(path)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
    }
    false
}

fn to_entry(
    path: &Path,
    walk_position: u32,
    metadata: &fs::Metadata,
    query: &CompiledQuery,
) -> FileSystemEntry {
    let path_string = path.to_string_lossy().to_string();
    let relative_path = path
        .strip_prefix(&query.root)
        .ok()
        .map(crate::text::slash_path)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| file_name(path));
    let name = file_name(path);
    let entry_type = if metadata.is_file() {
        FileKind::File
    } else if metadata.is_dir() {
        FileKind::Directory
    } else if metadata.file_type().is_symlink() {
        FileKind::Symlink
    } else {
        FileKind::Other
    };

    FileSystemEntry {
        path: path_string,
        relative_path,
        name,
        entry_type,
        size: Some(metadata.len() as i64),
        modified_time: metadata.modified().ok(),
        walk_position,
    }
}

fn parse_duration_option(value: Option<&str>, label: &str) -> Result<Option<u64>, Error> {
    value
        .map(|raw| {
            parse_duration(raw).ok_or_else(|| {
                Error::with_kind(
                    ErrorKind::InvalidInput,
                    format!(
                        "time.{label}=\"{raw}\" is not a relative duration the walk can apply. Use a whole number of minutes, hours, days or weeks like \"7d\", \"2h\", \"1w\", or \"3m\"."
                    ),
                )
            })
        })
        .transpose()
}

fn parse_duration(raw: &str) -> Option<u64> {
    // Split on the first non-digit at a char boundary. `split_at(len - 1)` would
    // panic on a multibyte trailing char (e.g. "7€") and only ever read a
    // single-byte unit.
    let unit_start = raw.char_indices().find(|(_, c)| !c.is_ascii_digit())?.0;
    let (number, unit) = raw.split_at(unit_start);
    let value = number.parse::<u64>().ok()?;
    // `checked_mul` returns None on overflow (release builds have no overflow
    // checks), routing absurd-but-valid numerics into the invalid-duration path
    // rather than wrapping silently.
    match unit {
        "m" => value.checked_mul(60),
        "h" => value.checked_mul(60 * 60),
        "d" => value.checked_mul(24 * 60 * 60),
        "w" => value.checked_mul(7 * 24 * 60 * 60),
        _ => None,
    }
}

fn parse_size_option(value: Option<&str>, label: &str) -> Result<Option<u64>, Error> {
    value
        .map(|raw| {
            parse_size(raw).ok_or_else(|| {
                Error::with_kind(
                    ErrorKind::InvalidInput,
                    format!("Invalid {label} value for local filesystem query: {raw}"),
                )
            })
        })
        .transpose()
}

fn parse_size(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let split_at = trimmed
        .char_indices()
        .find(|(_, ch)| !ch.is_ascii_digit() && *ch != '.')
        .map(|(idx, _)| idx)
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(split_at);
    let value = number.parse::<f64>().ok()?;
    let multiplier = match unit.to_ascii_lowercase().as_str() {
        "" | "b" | "c" => 1.0,
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((value * multiplier).round() as u64)
}

pub(crate) fn normalize_extensions(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().trim_start_matches('.').to_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

#[cfg(test)]
#[path = "fs_query_tests.rs"]
mod tests;
