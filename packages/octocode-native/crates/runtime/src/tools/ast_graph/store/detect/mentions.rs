//! The text-mention scan: one pass over the repository's text that says which
//! candidate file stems and export names it mentions, and where.
use super::mention_stream::scan_stream;
use super::*;
use crate::tools::ast_graph::classify::VENDORED_DIRS;
use crate::tools::page_memo::{
    PageMemo, Proven, Watch, directory_ignore_inputs, repository_markers,
};
use octocode_engine::stamp::{Link, Proof};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Largest file held whole; a larger one is read in windows ([`scan_stream`]).
pub(super) const MENTION_MAX_FILE: u64 = 2 << 20;

/// Directories the scan skips on top of the one default prune (build output,
/// deps, VCS, credential stores): vendored copies, which would mention every
/// name.
fn skipped_dirs() -> impl Iterator<Item = String> {
    VENDORED_DIRS.iter().map(|name| (*name).to_owned())
}

/// Per key, every file mentioning it with the 1-based line of its first
/// mention there, as `file:line`, files in path order.
pub(super) type Mentions = BTreeMap<String, Vec<String>>;

/// The mention scan's hits: file stems in path-like tokens, and export
/// names split by kind ([`scan_mentions`]).
#[derive(Default)]
pub(super) struct MentionHits {
    pub(super) stems: Mentions,
    /// Names in source code, or qualified (`index.run`, `checks/run`) in
    /// any text: a reference that may load the export.
    pub(super) references: Mentions,
    /// Names only as bare words of non-code text (prose, a list): context,
    /// never a reference.
    pub(super) words: Mentions,
    /// What proves the scan current, when it could be taken: every file and
    /// directory its walk met and the ignore inputs that shaped it.
    pub(super) proof: Option<Proof>,
}

impl MentionHits {
    /// About what the hits hold, for the memory budget.
    fn weight(&self) -> usize {
        [&self.stems, &self.references, &self.words]
            .into_iter()
            .flatten()
            .map(|(key, list)| {
                key.len()
                    + std::mem::size_of::<(String, Vec<String>)>()
                    + list
                        .iter()
                        .map(|mention| mention.len() + std::mem::size_of::<String>())
                        .sum::<usize>()
            })
            .sum()
    }
}

/// One text file the walk found.
struct Listed {
    path: std::path::PathBuf,
    /// Root-relative, `/`-separated: the name findings cite.
    rel: String,
    size: u64,
}

/// The text the scan may read under `root`, in path order, and the directories
/// the walk entered.
fn listed_files(root: &std::path::Path) -> (Vec<Listed>, Vec<std::path::PathBuf>) {
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    for entry in crate::tools::pruned_walk(root, skipped_dirs())
        .build()
        .flatten()
    {
        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
            dirs.push(entry.into_path());
        } else if entry.file_type().is_some_and(|kind| kind.is_file()) {
            let rel = entry.path().strip_prefix(root).map_or_else(
                |_| entry.path().to_string_lossy().into_owned(),
                octocode_engine::text::slash_path,
            );
            files.push(Listed {
                rel,
                size: entry.metadata().map_or(0, |meta| meta.len()),
                path: entry.into_path(),
            });
        }
    }
    files.sort_unstable_by(|a, b| a.rel.cmp(&b.rel));
    (files, dirs)
}

/// What proves a scan of `files`, walked from `root` since `started`, still
/// current: what a walk reads, stamped. `None` when one was written within the
/// settle window before it, so a later write might not move its stamp, or is an
/// ignore file no stamp can watch.
fn listing_proof(
    root: &std::path::Path,
    files: &[Listed],
    dirs: &[std::path::PathBuf],
    started: std::time::SystemTime,
) -> Option<Proof> {
    let mut sources = files
        .iter()
        .map(|file| file.path.clone())
        .chain(dirs.iter().cloned())
        .collect::<Vec<_>>();
    let mut rules = octocode_engine::search::walk::external_ignore_inputs(root);
    for dir in dirs {
        let inputs = directory_ignore_inputs(dir, root, |_| true);
        if inputs.untracked {
            return None;
        }
        sources.extend(inputs.files);
        rules.extend(inputs.external);
    }
    Watch {
        root,
        sources: &sources,
        rules: &rules,
        markers: &repository_markers(root),
    }
    .capture(Link::Follow, Some(started))
}

#[cfg(test)]
pub(super) fn scan_mentions(
    root: &std::path::Path,
    stems: &BTreeSet<String>,
    names: &BTreeSet<String>,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Option<MentionHits> {
    scan_listed(&listed_files(root).0, stems, names, stopped)
}

/// [`scan_listed`] of `root`, kept across calls while what it read stands
/// ([`listing_proof`]): an edit, an added or removed file and a `.gitignore`
/// change all miss, with no walk on a hit. A scan with no proof runs each time.
/// `None` when `stopped` ended the scan, which keeps nothing.
pub(super) fn memoized_mentions(
    root: &std::path::Path,
    stems: &BTreeSet<String>,
    names: &BTreeSet<String>,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Option<Arc<MentionHits>> {
    static MENTIONS: PageMemo<Proven<MentionHits>> = PageMemo::new();
    let mut key = octocode_engine::digest::HashWriter::default();
    key.update(root.as_os_str().as_encoded_bytes());
    for set in [stems, names] {
        key.update([0]);
        for item in set {
            key.update(item);
            key.update([0]);
        }
    }
    let key = hex::encode(key.finalize());
    if let Some(kept) = MENTIONS.current(&key, "") {
        return Some(kept);
    }
    let started = octocode_engine::stamp::settle_clock();
    let (files, dirs) = listed_files(root);
    let mut scanned = scan_listed(&files, stems, names, stopped)?;
    scanned.proof = listing_proof(root, &files, &dirs, started);
    let scanned = Arc::new(scanned);
    if let Some(proof) = &scanned.proof {
        let bytes = scanned.weight() + proof.weight();
        MENTIONS.put_proven(
            key,
            String::new(),
            bytes,
            proof.clone(),
            Arc::clone(&scanned),
        );
    }
    Some(scanned)
}

/// One pass over the repository's text: which candidate file stems appear as
/// path-like tokens (`'./worker.ts'`, `"src/tool.ts"`) or whole string
/// literals (`'worker'`, a name a loader builds a path from) and which export names
/// appear as identifiers, and where: every file, with its first line. This is
/// the SCARF/vulture "is it mentioned anywhere" safety check. A name in a
/// source file (any grammar the engine parses, or script-bearing markup:
/// [`octocode_engine::signatures::languages::may_reference_code`])
/// or qualified by a module path or object (`mod.run`, `index.run`) is a
/// reference; a bare word of other text (a README's "run") is only a word.
/// Workers hold one file's text at a time, or one window of a larger file.
/// `None` when `stopped` ended the scan.
fn scan_listed(
    files: &[Listed],
    stems: &BTreeSet<String>,
    names: &BTreeSet<String>,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Option<MentionHits> {
    let keys = Keys::new(stems, names);
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(files.len().max(1));
    let found = std::thread::scope(|scope| {
        let workers = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut hits = Vec::new();
                    while let Some(file) = files
                        .get(next.fetch_add(1, Ordering::Relaxed))
                        .filter(|_| !stopped())
                    {
                        let local = scan_file(file, &keys, stopped);
                        if local.iter().any(|kind| !kind.is_empty()) {
                            hits.push((file.rel.as_str(), local));
                        }
                    }
                    hits
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .flat_map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect::<Vec<_>>()
    });
    (!stopped()).then(|| assemble(found))
}

/// The hits of one file, none when it cannot be read as text.
fn scan_file<'k>(
    file: &Listed,
    keys: &Keys<'k>,
    stopped: &(dyn Fn() -> bool + Sync),
) -> [BTreeMap<&'k str, usize>; 3] {
    let extension = octocode_engine::text::extension_of(&file.rel, true, "");
    let code = octocode_engine::signatures::languages::may_reference_code(&extension);
    let limit = if file.size <= MENTION_MAX_FILE {
        MENTION_MAX_FILE
    } else {
        match large_file(file) {
            Some(Large::Windows(source)) => return scan_stream(source, code, keys, stopped),
            Some(Large::Whole) => file.size,
            None => return Default::default(),
        }
    };
    let Ok(source) = octocode_engine::text::read_regular(&file.path, limit) else {
        return Default::default();
    };
    // The one source admission: binary refused, any text decoded as the
    // reader tools show it.
    let Ok(decoded) = source.admit(octocode_engine::text::Decoding::Lenient) else {
        return Default::default();
    };
    scan_text(&decoded.text, code, keys)
}

/// How a file past [`MENTION_MAX_FILE`] is read.
enum Large {
    Windows(std::fs::File),
    /// BOM-marked UTF-16 decodes only as a whole.
    Whole,
}

fn large_file(file: &Listed) -> Option<Large> {
    use std::io::{Read, Seek};
    let (mut source, _) = octocode_engine::text::open_regular(&file.path).ok()?;
    let mut mark = [0; 2];
    let marked = source.read_exact(&mut mark).is_ok();
    source.rewind().ok()?;
    Some(if marked && octocode_engine::text::has_utf16_bom(&mark) {
        Large::Whole
    } else {
        Large::Windows(source)
    })
}

/// Per key, its mentions: files in path order, so each key lists its files in
/// path order whatever order the workers read them in.
fn assemble(mut files: Vec<(&str, [BTreeMap<&str, usize>; 3])>) -> MentionHits {
    files.sort_unstable_by(|a, b| a.0.cmp(b.0));
    let mut hits: [Mentions; 3] = Default::default();
    for (rel, local) in &files {
        for (mentions, mine) in hits.iter_mut().zip(local) {
            for (key, line) in mine {
                match mentions.get_mut(*key) {
                    Some(list) => list.push(format!("{rel}:{line}")),
                    None => {
                        mentions.insert((*key).to_owned(), vec![format!("{rel}:{line}")]);
                    }
                }
            }
        }
    }
    let [stems, references, words] = hits;
    MentionHits {
        stems,
        references,
        words,
        proof: None,
    }
}

/// The `file:line` mentions of `key` in files other than `subject`.
pub(super) fn mentions_outside(hits: &Mentions, key: &str, subject: &str) -> Vec<String> {
    hits.get(key)
        .into_iter()
        .flatten()
        .filter(|mention| {
            mention
                .rsplit_once(':')
                .is_none_or(|(file, _)| file != subject)
        })
        .cloned()
        .collect()
}
