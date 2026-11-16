//! Bounded probe searches of an empty page: other spellings of a literal
//! miss ([`alternates`]), and matches in the paths the walk skipped.
use crate::tools::local_search::local_search_query;
use crate::tools::local_search::local_search_query::View;

use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::policy::prune::DefaultsFlag;
use crate::response::result::ToolError;
use crate::tools::empty_walk::{Cause, Globs, Hits, ProbeWork, Tagged, Verdict, Walk, diagnose};
use crate::tools::leads::skipped::{SkippedEntry, skipped_cause, skipped_paths};
use octocode_contract_types::{LocalSearchQuery, SkippedPaths};
use octocode_engine::search::text_search::{
    Stream, Streaming, TextSearchPathFilter, search_cancellable, search_stream,
};
use octocode_engine::types::{PatternSyntax, SearchOutput, SearchSort, TextSearchOptions};
use serde_json::{Value, json};
use std::sync::Arc;

mod alternates;
pub(super) use alternates::Alternates;
#[cfg(test)]
mod tests;

/// What the re-run of an empty search over its skipped paths found: the
/// files, each tagged with the causes that hid it, and what the path policy
/// withheld from that walk (symlinks it does not follow, credential files).
pub(super) struct Lifted {
    tagged: Tagged,
    withheld: crate::policy::discovery::Withheld,
    /// Files that walk met that are binary from their leading bytes: never
    /// text it searched.
    binary_skipped: u32,
}

/// Re-run an empty search over what the defaults leave out (ignored and
/// hidden entries, the default-excluded directories), each file found
/// tagged with the causes that hid it ([`probe_lifted`]). `searched` is what
/// the empty scan already searched, when its record can serve: those files hold
/// nothing the page could list, so they are not read again.
pub(super) fn probe_unsearched(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    search_options: &TextSearchOptions,
    searched: Option<&[std::path::PathBuf]>,
    cancel: &impl CancellationCheck,
) -> Result<Lifted, ()> {
    let walk = query_walk(query, paths.global_ignore());
    probe_lifted(
        search_options,
        walk.lifting(&walk.skipping()),
        paths,
        searched,
        cancel,
    )
}

/// A probe walk's path filter: the walk's verdicts, less the files already
/// searched. Directories still walk: a skipped path lies below them.
struct BeyondSearched {
    walk: Arc<crate::policy::discovery::WalkAdmission<'static>>,
    searched: std::collections::HashSet<std::path::PathBuf>,
}

impl TextSearchPathFilter for BeyondSearched {
    fn allows(&self, path: &std::path::Path, is_dir: bool) -> bool {
        (is_dir || !self.searched.contains(path)) && self.walk.allows(path, is_dir)
    }

    fn allows_entry(&self, path: &std::path::Path, file_type: Option<std::fs::FileType>) -> bool {
        (file_type.is_some_and(|kind| kind.is_dir()) || !self.searched.contains(path))
            && self.walk.allows_entry(path, file_type)
    }

    fn skipped_hidden(&self, path: &std::path::Path, is_dir: bool) {
        self.walk.skipped_hidden(path, is_dir);
    }

    fn skipped_ignored(&self, path: &std::path::Path, is_dir: bool) {
        self.walk.skipped_ignored(path, is_dir);
    }

    fn skipped_symlink(&self, path: &std::path::Path) {
        self.walk.skipped_symlink(path);
    }
}

/// Whether a walk's ledger names nothing: no skipped path a flag lifts, nothing
/// the path policy withheld. A relaxed walk then has nothing to add, so an
/// empty page needs no check of its skipped paths.
pub(super) fn names_nothing(skipped: &crate::policy::discovery::WalkSkips) -> bool {
    skipped.counts().is_empty() && skipped.withheld.total() == 0
}

/// The walk the query ran: the causes it skipped paths by.
fn query_walk(
    query: &LocalSearchQuery,
    global: octocode_engine::search::walk::GlobalIgnore,
) -> Walk {
    Walk {
        global,
        ignored: local_search_query::include_ignored(query) == Some(true),
        hidden: local_search_query::include_hidden(query) == Some(true),
        default_excludes: local_search_query::default_excludes(query).defaults(),
    }
}

/// Say which skipped paths the re-run of an empty search
/// ([`probe_unsearched`]) found a match in: the hint names the rule, entry
/// or directory, and the lead searches again with only those lifted.
/// Default-excluded directories the re-run could not finish are named with
/// the lead that searches them. Returns whether skipped paths were shown to
/// hold a match, or what the check searched when they hold none.
pub(super) fn disclose_unsearched(
    query: &LocalSearchQuery,
    search_options: &TextSearchOptions,
    lifted: Result<Lifted, ()>,
    skipped: Skipped<'_>,
    next: &mut Option<Value>,
    hints: &mut Vec<String>,
) -> Result<Unsearched, ToolError> {
    let walk = query_walk(query, search_options.global_ignore);
    let root = std::path::PathBuf::from(&search_options.path);
    let verdict = diagnose(&root, walk, |relaxed| {
        Ok::<_, std::convert::Infallible>(lifted.as_ref().map_or_else(
            |()| Hits {
                cut: true,
                ..Hits::default()
            },
            |lifted| lifted.tagged.find(relaxed, None),
        ))
    });
    let Ok(verdict) = verdict;
    if let Verdict::Absent = verdict {
        return Ok(Unsearched {
            checked: lifted
                .as_ref()
                .ok()
                .and_then(|lifted| absent(&skipped, lifted)),
            ..Unsearched::default()
        });
    }
    let subject = if local_search_query::view(query) == View::FilesWithout {
        ["file that lacks a match", "files that lack a match"]
    } else {
        ["file that matches", "files that match"]
    };
    let named = skipped.walk.counts();
    let Some(disclosure) = crate::tools::leads::skipped::disclose(
        &verdict,
        subject,
        crate::tools::leads::skipped::Unchecked {
            causes: &skipped.causes,
            named: &named,
        },
    ) else {
        return Ok(Unsearched::default());
    };
    insert_lead(
        next,
        crate::tools::leads::skipped::lead(
            crate::id::ToolId::LocalSearch,
            query,
            &disclosure.causes,
        ),
    );
    // A finished check names the rules behind the files it found. One that ran
    // out of time found nothing to name: the walk's own skips stand, counted by
    // cause, and the lead reaches them.
    let named = match &verdict {
        Verdict::Skipped(diagnosis) => diagnosis.skips().to_vec(),
        _ => Vec::new(),
    };
    hints.insert(0, disclosure.text);
    Ok(Unsearched {
        may_hold_matches: true,
        observed: observed_paths(&named)?,
        checked: None,
    })
}

/// The check of the skipped paths searched them and found nothing, so no
/// lead lifts them: what it searched, by cause, what the path policy
/// withheld from its walk beyond the page's own, and the binary files it met
/// (never text it searched), all stated as counts in the empty page's one
/// absence sentence. Security-policy directories (`.git/`) are left out of
/// the withheld count: no setting lifts them, so they hide nothing a search
/// could reach.
fn absent(skipped: &Skipped<'_>, lifted: &Lifted) -> Option<Checked> {
    let paths = skipped.walk.counts();
    if paths.is_empty() {
        return None;
    }
    let beyond = lifted.withheld.beyond(&skipped.walk.withheld);
    let policy_dirs: usize = beyond.dirs.values().sum();
    Some(Checked {
        paths,
        withheld: beyond.total().saturating_sub(policy_dirs),
        binary_skipped: lifted.binary_skipped,
    })
}

/// What the check of an empty page's skipped paths proved: they hold nothing
/// the page could list.
pub(super) struct Checked {
    /// The skipped paths it searched, counted by cause (`1 ignored, 23 hidden`).
    pub(super) paths: String,
    /// Entries its walk withheld beyond the page's own, less security-policy
    /// directories.
    pub(super) withheld: usize,
    /// Files its walk met that are binary from their leading bytes.
    pub(super) binary_skipped: u32,
}

/// What a page's walk skipped, by cause, for the empty page's check of it.
pub(super) struct Skipped<'a> {
    pub(super) walk: &'a crate::policy::discovery::WalkSkips,
    /// The causes it skipped paths by: what a lead over them lifts.
    pub(super) causes: Vec<Cause>,
}

/// What the empty page's check of the skipped paths found.
#[derive(Default)]
pub(super) struct Unsearched {
    /// Skipped paths hold a match, or may: the check did not finish.
    pub(super) may_hold_matches: bool,
    /// The skipped paths the check named, each with its rule.
    pub(super) observed: Option<SkippedPaths>,
    /// The check found nothing in them.
    pub(super) checked: Option<Checked>,
}

fn insert_lead(next: &mut Option<Value>, lead: Value) {
    if let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut() {
        map.insert("includeSkipped".into(), lead);
    }
}

/// Every skipped path a check named, with its rule. Findings are samples
/// (`sampled`); the `includeSkipped` lead searches the entire skipped scope.
fn observed_paths(
    skipped: &[crate::tools::empty_walk::Skip],
) -> Result<Option<SkippedPaths>, ToolError> {
    skipped_paths(
        skipped.iter().map(|skip| {
            SkippedEntry::path(
                octocode_engine::text::slash_path(&skip.entry),
                skipped_cause(skip.cause),
            )
            .because(skip.reason.clone())
        }),
        true,
        true,
    )
}

#[cfg(test)]
thread_local! {
    static PROBE_WALKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Probe walks this thread ran.
#[cfg(test)]
fn probe_walks() -> usize {
    PROBE_WALKS.with(std::cell::Cell::get)
}

/// Run a bounded search over the skipped scope with policy enforcement: one
/// walker in traversal order, charged per admitted entry and the bytes of
/// each admitted file ([`ProbeWork`]), so the same tree is cut at the same
/// entry on every run.
fn probe_walked(
    options: TextSearchOptions,
    paths: &PathPolicy,
    defaults: bool,
    searched: Option<&[std::path::PathBuf]>,
    cancel: &impl CancellationCheck,
) -> Option<(
    octocode_engine::types::TextSearchResult,
    bool,
    crate::policy::discovery::Withheld,
)> {
    let walk = Arc::new(crate::policy::discovery::WalkAdmission::search(
        paths.clone(),
        std::path::PathBuf::from(&options.path),
        defaults,
        options.include.as_deref().unwrap_or_default(),
    ));
    let admitted: Arc<dyn TextSearchPathFilter> = match searched {
        Some(searched) => Arc::new(BeyondSearched {
            walk: Arc::clone(&walk),
            searched: searched.iter().cloned().collect(),
        }),
        None => walk.clone(),
    };
    let work = Arc::new(ProbeWork::new());
    let found = charged_search(options, admitted, &work, cancel)?;
    Some((found, work.cut(), walk.skipped().withheld))
}

/// `options`' search over what `admitted` admits, each admitted entry charged
/// to `work`, in one walker's order: the same tree is cut at the same entry on
/// every run. `None` when the search cannot run.
fn charged_search(
    mut options: TextSearchOptions,
    admitted: Arc<dyn TextSearchPathFilter>,
    work: &Arc<ProbeWork>,
    cancel: &impl CancellationCheck,
) -> Option<octocode_engine::types::TextSearchResult> {
    #[cfg(test)]
    PROBE_WALKS.with(|walks| walks.set(walks.get() + 1));
    let filter = Arc::new(Charged {
        admitted,
        work: Arc::clone(work),
    });
    // The walk order, read one entry at a time: the parallel walk admits
    // entries in no fixed order. (A PCRE2 search lists its candidates first
    // and has no stream.)
    let stop = || cancel.check().is_err() || work.cut();
    let mut found = if options.syntax == PatternSyntax::Pcre2 {
        options.sort = SearchSort::Traversal;
        search_cancellable(options, filter, &stop).ok()?
    } else {
        let stream = Stream {
            bind: String::new(),
            from: 0,
            pin: None,
            enough: Box::new(|_| false),
        };
        match search_stream(options, filter, &stop, stream).ok()? {
            Streaming::Streamed(streamed) => streamed.result,
            Streaming::Stale => return None,
        }
    };
    // Named by path, as a whole walk would list them.
    found.files.sort_by(|a, b| a.path.cmp(&b.path));
    Some(found)
}

/// Where the same search finds a match above an empty page's searched path.
pub(super) enum Ancestor {
    /// The first folder above the searched path with a match, as rows name
    /// paths: how many files match there, a floor when the probe was cut
    /// inside that folder; the narrower folder under it that holds every
    /// match, when the probe finished it; and the causes its walk lifted
    /// because they keep the searched path out of it (the searched path is
    /// ignored, hidden or default-excluded there).
    Found {
        dir: String,
        holding: Option<String>,
        files: usize,
        floor: bool,
        lifted: Vec<Cause>,
    },
    /// The search finds nothing anywhere in the tree the path lies in
    /// ([`PathPolicy::tree_root`]): the workspace (`None`), or the named one
    /// (a clone's checkout).
    Nowhere { tree: Option<String> },
    /// The probe was cut, or could not run, before any folder answered.
    Unknown,
}

/// The narrowest folder above `root` (the empty page's searched file or
/// directory) where the same search finds a match. One
/// probe's work ([`ProbeWork`]) is shared by the walks of each folder up to the
/// top of the tree `root` lies in ([`PathPolicy::tree_root`]: the workspace,
/// a clone's checkout), each walking only what the folder below it did not, so
/// every entry is read once; it stops at the first folder with a match and
/// names the narrowest folder under it that holds every match. Each folder is
/// walked as the search reached `root`: a cause that would keep `root` out of
/// that folder's walk (`root` is ignored there, say) is lifted for it, so the
/// folders `root` sits among are searched as `root` was.
pub(super) fn matching_ancestor(
    options: &TextSearchOptions,
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    root: &std::path::Path,
    cancel: &impl CancellationCheck,
) -> Ancestor {
    let output = if options.output == SearchOutput::FilesWithoutMatch {
        SearchOutput::FilesWithoutMatch
    } else {
        SearchOutput::FilesWithMatch
    };
    let Some(top) = paths.tree_root(root).filter(|top| top.as_path() != root) else {
        return Ancestor::Unknown;
    };
    let asked = query_walk(query, paths.global_ignore());
    let work = Arc::new(ProbeWork::new());
    let mut searched = root.to_path_buf();
    while let Some(dir) = searched.parent().map(std::path::Path::to_path_buf) {
        if !dir.starts_with(&top) {
            return Ancestor::Unknown;
        }
        let lifted = crate::tools::empty_walk::keeping_out(
            &dir,
            root,
            root.is_dir(),
            &asked.skipping(),
            asked.global,
        );
        let reach = asked.lifting(&lifted);
        let mut level = options.probe(output);
        level.path = dir.to_string_lossy().into_owned();
        level.ignore = reach.policy();
        let walk = Arc::new(crate::policy::discovery::WalkAdmission::search(
            paths.clone(),
            dir.clone(),
            reach.default_excludes,
            level.include.as_deref().unwrap_or_default(),
        ));
        let admitted = Arc::new(Outside {
            walk,
            searched: searched.clone(),
        });
        let Some(found) = charged_search(level, admitted, &work, cancel) else {
            return Ancestor::Unknown;
        };
        let cut = work.cut() || !found.stats.cap_reasons.is_empty();
        if !found.files.is_empty() {
            // A finished walk names every match, so the folder that holds them
            // all is the lead; a cut one may have more past the cut, anywhere
            // in this folder.
            let holding = (!cut)
                .then(|| common_dir(found.files.iter().map(|file| dir.join(&file.path))))
                .flatten()
                .filter(|holding| holding.starts_with(&dir) && *holding != dir);
            return Ancestor::Found {
                dir: paths.shown(&dir),
                holding: holding.map(|holding| paths.shown(holding)),
                files: found.files.len(),
                floor: cut,
                lifted,
            };
        }
        if cut {
            return Ancestor::Unknown;
        }
        if dir == top {
            return Ancestor::Nowhere {
                tree: paths
                    .workspace_relative(&dir)
                    .is_none()
                    .then(|| paths.shown(&dir)),
            };
        }
        searched = dir;
    }
    Ancestor::Unknown
}

/// The narrowest directory that holds every one of `files`.
fn common_dir(files: impl Iterator<Item = std::path::PathBuf>) -> Option<std::path::PathBuf> {
    let mut common: Option<std::path::PathBuf> = None;
    for file in files {
        let mut holding = match common {
            Some(holding) => holding,
            None => file.parent()?.to_path_buf(),
        };
        while !file.starts_with(&holding) {
            holding = holding.parent()?.to_path_buf();
        }
        common = Some(holding);
    }
    common
}

/// A probe walk's path filter that leaves out the path the walk below it
/// already searched: the walk does not enter it.
struct Outside {
    walk: Arc<crate::policy::discovery::WalkAdmission<'static>>,
    searched: std::path::PathBuf,
}

impl TextSearchPathFilter for Outside {
    fn allows(&self, path: &std::path::Path, is_dir: bool) -> bool {
        path != self.searched && self.walk.allows(path, is_dir)
    }

    fn allows_entry(&self, path: &std::path::Path, file_type: Option<std::fs::FileType>) -> bool {
        path != self.searched && self.walk.allows_entry(path, file_type)
    }

    fn skipped_hidden(&self, path: &std::path::Path, is_dir: bool) {
        self.walk.skipped_hidden(path, is_dir);
    }

    fn skipped_ignored(&self, path: &std::path::Path, is_dir: bool) {
        self.walk.skipped_ignored(path, is_dir);
    }

    fn skipped_symlink(&self, path: &std::path::Path) {
        self.walk.skipped_symlink(path);
    }
}

/// A probe walk's path filter that charges each entry it admits to the
/// probe's work, a file by its length: past the work it admits nothing, and
/// the walk's stop check ends it.
struct Charged {
    admitted: Arc<dyn TextSearchPathFilter>,
    work: Arc<ProbeWork>,
}

impl TextSearchPathFilter for Charged {
    fn allows(&self, path: &std::path::Path, is_dir: bool) -> bool {
        self.admitted.allows(path, is_dir)
    }

    fn allows_entry(&self, path: &std::path::Path, file_type: Option<std::fs::FileType>) -> bool {
        if !self.admitted.allows_entry(path, file_type) {
            return false;
        }
        let bytes = if file_type.is_some_and(|kind| kind.is_file()) {
            std::fs::metadata(path).map_or(0, |meta| meta.len())
        } else {
            0
        };
        self.work.charge(bytes)
    }

    fn skipped_hidden(&self, path: &std::path::Path, is_dir: bool) {
        self.admitted.skipped_hidden(path, is_dir);
    }

    fn skipped_ignored(&self, path: &std::path::Path, is_dir: bool) {
        self.admitted.skipped_ignored(path, is_dir);
    }

    fn skipped_symlink(&self, path: &std::path::Path) {
        self.admitted.skipped_symlink(path);
    }
}

/// The prefix of `files`, in their order, that fits one probe's work (each
/// charged by its length), and whether files were left out.
fn within_work(files: &[std::path::PathBuf]) -> (&[std::path::PathBuf], bool) {
    let work = ProbeWork::new();
    let fits = files
        .iter()
        .take_while(|file| work.charge(std::fs::metadata(file).map_or(0, |meta| meta.len())))
        .count();
    (&files[..fits], fits < files.len())
}

/// The files `options`' search finds under the fully relaxed `walk` (files
/// only), tagged with the causes that hide each ([`Tagged::found`]). A search
/// that cannot run is `Err`; one cut by its deadline keeps what it found and
/// answers every relaxed walk as cut.
fn probe_lifted(
    options: &TextSearchOptions,
    walk: Walk,
    paths: &PathPolicy,
    searched: Option<&[std::path::PathBuf]>,
    cancel: &impl CancellationCheck,
) -> Result<Lifted, ()> {
    // The files the row's view lists: those without a match for a
    // `filesWithout` row, else those with one.
    let mut lifted = options.probe(if options.output == SearchOutput::FilesWithoutMatch {
        SearchOutput::FilesWithoutMatch
    } else {
        SearchOutput::FilesWithMatch
    });
    lifted.ignore = octocode_engine::types::IgnorePolicy {
        include_ignored: walk.ignored,
        include_hidden: walk.hidden,
    };
    let root = std::path::PathBuf::from(&options.path);
    let globs = Globs {
        include: options.include.as_deref().unwrap_or_default(),
        exclude: options.exclude.as_deref().unwrap_or_default(),
    };
    let (searched, cut, withheld) =
        probe_walked(lifted, paths, walk.default_excludes, searched, cancel).ok_or(())?;
    // Refusing the final entry can end the walk before its next cancellation
    // check. The probe's own cut flag still makes the unchecked scope unknown.
    let cut = cut || !searched.stats.cap_reasons.is_empty();
    let binary_skipped = searched.stats.skipped_binary_count.unwrap_or(0);
    let tagged = Tagged::found(
        &root,
        paths.global_ignore(),
        globs,
        searched.files.iter().map(|file| root.join(&file.path)),
        cut,
    )
    .map_err(|_| ())?;
    Ok(Lifted {
        tagged,
        withheld,
        binary_skipped,
    })
}
