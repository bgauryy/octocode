//! The text-mention scan: one pass over the repository's text that says which
//! candidate file stems and export names it mentions, and where.
use super::*;
use crate::tools::ast_graph::classify::VENDORED_DIRS;
use crate::tools::page_memo::PageMemo;
use octocode_engine::stamp::SourceStamp;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) const MENTION_MAX_FILE: u64 = 2 << 20;

/// Directories the scan skips on top of the one default prune (build output,
/// deps, VCS, credential stores): vendored copies, which would mention every
/// name.
fn skipped_dirs() -> impl Iterator<Item = String> {
    VENDORED_DIRS.iter().map(|name| (*name).to_owned())
}

/// What the mention scan did not read, so "no mention" stays a scoped claim.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct MentionGaps {
    pub(super) oversized: u64,
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

/// One text file the walk found, stamped before it is read.
struct Listed {
    path: std::path::PathBuf,
    /// Root-relative, `/`-separated: the name findings cite.
    rel: String,
    size: u64,
    stamp: Option<SourceStamp>,
    /// Whether the stamp may stand in for the bytes ([`SourceStamp::settled`]).
    settled: bool,
}

/// The text the scan may read under `root`, in path order, each file stamped
/// before any is read.
fn listed_files(root: &std::path::Path) -> Vec<Listed> {
    let now = octocode_engine::stamp::settle_clock();
    let mut files = crate::tools::pruned_walk(root, skipped_dirs())
        .build()
        .flatten()
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .map(|entry| {
            let meta = entry.metadata().ok();
            let rel = entry.path().strip_prefix(root).map_or_else(
                |_| entry.path().to_string_lossy().into_owned(),
                |path| path.to_string_lossy().replace('\\', "/"),
            );
            Listed {
                rel,
                size: meta.as_ref().map_or(0, std::fs::Metadata::len),
                stamp: meta.as_ref().and_then(SourceStamp::of),
                settled: meta
                    .as_ref()
                    .is_some_and(|meta| SourceStamp::settled(meta, now).is_some()),
                path: entry.into_path(),
            }
        })
        .collect::<Vec<_>>();
    files.sort_unstable_by(|a, b| a.rel.cmp(&b.rel));
    files
}

#[cfg(test)]
pub(super) fn scan_mentions(
    root: &std::path::Path,
    stems: &BTreeSet<String>,
    names: &BTreeSet<String>,
) -> (MentionHits, MentionGaps) {
    scan_listed(&listed_files(root), stems, names)
}

/// [`scan_listed`] of `root`, kept across calls while what it read stands:
/// the key is the key sets and every listed file's stamp, so an edit, an added
/// or removed file and a `.gitignore` change that alters the listing all miss.
/// Only a listing of settled stamps is kept; an unsettled one scans each time.
pub(super) fn memoized_mentions(
    root: &std::path::Path,
    stems: &BTreeSet<String>,
    names: &BTreeSet<String>,
) -> Arc<(MentionHits, MentionGaps)> {
    static MENTIONS: PageMemo<Arc<(MentionHits, MentionGaps)>> = PageMemo::new();
    let files = listed_files(root);
    let mut key = octocode_engine::digest::HashWriter::default();
    key.update(root.as_os_str().as_encoded_bytes());
    for set in [stems, names] {
        key.update([0]);
        for item in set {
            key.update(item);
            key.update([0]);
        }
    }
    for file in &files {
        key.update(&file.rel);
        key.update([0]);
        let stamp = file.stamp.unwrap_or_default();
        for part in [stamp.size, stamp.modified_ns, stamp.changed_ns, stamp.inode] {
            key.update(part.to_le_bytes());
        }
    }
    let key = hex::encode(key.finalize());
    let provable = files.iter().all(|file| file.settled);
    if provable && let Some(kept) = MENTIONS.get(&key, "", Arc::clone) {
        return kept;
    }
    let scanned = Arc::new(scan_listed(&files, stems, names));
    if provable {
        MENTIONS.put(key, String::new(), scanned.0.weight(), Arc::clone(&scanned));
    }
    scanned
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
fn scan_listed(
    files: &[Listed],
    stems: &BTreeSet<String>,
    names: &BTreeSet<String>,
) -> (MentionHits, MentionGaps) {
    let keys = Keys::new(stems, names);
    // A file past the size bound is counted. Workers hold one file's text at a
    // time, so the text behind them has no bound to keep.
    let (readable, oversized): (Vec<_>, Vec<_>) =
        files.iter().partition(|file| file.size <= MENTION_MAX_FILE);
    let gaps = MentionGaps {
        oversized: oversized.len() as u64,
    };
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(readable.len().max(1));
    let found = std::thread::scope(|scope| {
        let workers = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut hits = Vec::new();
                    while let Some(file) = readable.get(next.fetch_add(1, Ordering::Relaxed)) {
                        let local = scan_file(file, &keys);
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
    (assemble(found), gaps)
}

/// The hits of one file, none when it cannot be read as text.
fn scan_file<'k>(file: &Listed, keys: &Keys<'k>) -> [BTreeMap<&'k str, usize>; 3] {
    let Ok(source) = octocode_engine::text::read_regular(&file.path, MENTION_MAX_FILE) else {
        return Default::default();
    };
    // The one source admission: binary refused, any text decoded as the
    // reader tools show it.
    let Ok(decoded) = source.admit(octocode_engine::text::Decoding::Lenient) else {
        return Default::default();
    };
    let extension = octocode_engine::text::extension_of(&file.rel, true, "");
    let code = octocode_engine::signatures::languages::may_reference_code(&extension);
    scan_text(&decoded.text, code, keys)
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
