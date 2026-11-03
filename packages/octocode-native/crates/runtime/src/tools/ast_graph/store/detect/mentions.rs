//! The text-mention scan: one pass over the repository's text that says which
//! candidate file stems and export names it mentions, and where.
use super::*;
use crate::tools::ast_graph::classify::VENDORED_DIRS;

pub(super) const MENTION_MAX_FILE: u64 = 2 << 20;
pub(super) const MENTION_MAX_TOTAL: u64 = 512 << 20;

/// Directories the scan skips on top of the shared syntax-visible prune
/// (build output, deps, VCS, credential stores): the environment and
/// tool-output directories the ingest excludes, and vendored copies, which
/// would mention every name. Both lists are the graph's own, so the scan
/// reads what the graph builds from.
fn skipped_dirs() -> impl Iterator<Item = String> {
    super::super::INGEST_EXCLUDES
        .iter()
        .chain(VENDORED_DIRS)
        .map(|name| (*name).to_owned())
}

/// What the mention scan did not read, so "no mention" stays a scoped claim.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct MentionGaps {
    pub(super) truncated: bool,
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

/// One pass over the repository's text: which candidate file stems appear as
/// path-like tokens (`'./worker.ts'`, `"src/tool.ts"`) or whole string
/// literals (`'worker'`, a name a loader builds a path from) and which export names
/// appear as identifiers, and where: every file, with its first line. This is
/// the SCARF/vulture "is it mentioned anywhere" safety check. A name in a
/// source file (any grammar the engine parses, or script-bearing markup:
/// [`octocode_engine::signatures::languages::may_reference_code`])
/// or qualified by a module path or object (`mod.run`, `index.run`) is a
/// reference; a bare word of other text (a README's "run") is only a word.
pub(super) fn scan_mentions(
    root: &std::path::Path,
    stems: &BTreeSet<String>,
    names: &BTreeSet<String>,
) -> (MentionHits, MentionGaps) {
    use std::collections::HashSet;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    // Hashed views of the key sets: a token lookup is the inner loop. Keys
    // borrow from the sets, so a hit allocates nothing.
    let stem_set: HashSet<&str> = stems.iter().map(String::as_str).collect();
    let name_set: HashSet<&str> = names.iter().map(String::as_str).collect();
    // One file's hits: per kind (stems, references, words), per key, its
    // first line in the file.
    type FileHits<'k> = (String, [BTreeMap<&'k str, usize>; 3]);
    /// Each walker thread keeps its files' hits and hands them over once,
    /// when the walk drops it: no lock per file.
    struct Sink<'s, 'k> {
        files: Vec<FileHits<'k>>,
        shared: &'s Mutex<Vec<FileHits<'k>>>,
    }
    impl Drop for Sink<'_, '_> {
        fn drop(&mut self) {
            let mut shared = self
                .shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            shared.append(&mut self.files);
        }
    }
    let found = Mutex::new(Vec::<FileHits<'_>>::new());
    // Bytes read so far; a file that would pass the total ends the scan.
    let spent = AtomicU64::new(0);
    let truncated = AtomicBool::new(false);
    let oversized = AtomicU64::new(0);
    let walker = crate::tools::pruned_walk(root, skipped_dirs()).build_parallel();
    walker.run(|| {
        let (stem_set, name_set) = (&stem_set, &name_set);
        let spent = &spent;
        let truncated = &truncated;
        let oversized = &oversized;
        let mut sink = Sink {
            files: Vec::new(),
            shared: &found,
        };
        Box::new(move |entry| {
            let Ok(entry) = entry else {
                return ignore::WalkState::Continue;
            };
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                return ignore::WalkState::Continue;
            }
            let size = entry.metadata().map_or(0, |meta| meta.len());
            if size > MENTION_MAX_FILE {
                oversized.fetch_add(1, Ordering::Relaxed);
                return ignore::WalkState::Continue;
            }
            if spent
                .fetch_add(size, Ordering::Relaxed)
                .saturating_add(size)
                > MENTION_MAX_TOTAL
            {
                truncated.store(true, Ordering::Relaxed);
                return ignore::WalkState::Quit;
            }
            let Ok(source) = octocode_engine::text::read_source(entry.path(), MENTION_MAX_FILE)
            else {
                return ignore::WalkState::Continue;
            };
            // The one source admission: binary refused, any text decoded as
            // the reader tools show it.
            let Ok(decoded) = source.admit(octocode_engine::text::Decoding::Lenient) else {
                return ignore::WalkState::Continue;
            };
            let rel = entry.path().strip_prefix(root).map_or_else(
                |_| entry.path().to_string_lossy().into_owned(),
                |path| path.to_string_lossy().replace('\\', "/"),
            );
            let extension = octocode_engine::text::extension_of(&rel, true, "");
            let code = octocode_engine::signatures::languages::may_reference_code(&extension);
            let text = decoded.text;
            let mut local: [BTreeMap<&str, usize>; 3] = Default::default();
            for (index, line) in text.lines().enumerate() {
                for (offset, token) in tokens(line) {
                    if token.contains(['/', '.']) {
                        let last = token.rsplit('/').next().unwrap_or(token);
                        let stem = last.split('.').next().unwrap_or(last);
                        if let Some(&stem) = stem_set.get(stem) {
                            local[0].entry(stem).or_insert(index + 1);
                        }
                    } else if let Some(&stem) = stem_set.get(token)
                        && quoted(line, offset, token)
                    {
                        // A bare name as a whole string literal (`'worker'`)
                        // is a file a loader may build a path from.
                        local[0].entry(stem).or_insert(index + 1);
                    }
                    let mut at = 0;
                    for part in token.split(['.', '/', '-', '@']) {
                        let start = at;
                        at += part.len() + 1;
                        let Some(&name) = name_set.get(part) else {
                            continue;
                        };
                        let qualified =
                            token[..start].strip_suffix(['.', '/']).is_some_and(|head| {
                                head.ends_with(|c: char| c.is_alphanumeric() || "_$".contains(c))
                            });
                        let kind = if code || qualified { 1 } else { 2 };
                        local[kind].entry(name).or_insert(index + 1);
                    }
                }
            }
            if local.iter().any(|hits| !hits.is_empty()) {
                sink.files.push((rel, local));
            }
            ignore::WalkState::Continue
        })
    });
    let mut files = found
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Files in path order, so each key lists its files in path order
    // whatever order the parallel walk visited them in.
    files.sort_unstable_by(|a, b| a.0.cmp(&b.0));
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
    (
        MentionHits {
            stems,
            references,
            words,
        },
        MentionGaps {
            truncated: truncated.load(Ordering::Relaxed),
            oversized: oversized.load(Ordering::Relaxed),
        },
    )
}

/// `line`'s runs of name and path characters, with their byte offsets.
fn tokens(line: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut start = None;
    line.char_indices()
        .chain(std::iter::once((line.len(), ' ')))
        .filter_map(move |(at, c)| {
            if c.is_alphanumeric() || "_$./@-".contains(c) {
                start.get_or_insert(at);
                None
            } else {
                start.take().map(|from| (from, &line[from..at]))
            }
        })
}

/// Whether `token` at `offset` is a whole string literal: the same quote
/// right before and right after it.
fn quoted(line: &str, offset: usize, token: &str) -> bool {
    let before = line[..offset].chars().next_back();
    let after = line[offset + token.len()..].chars().next();
    matches!(before, Some('\'' | '"' | '`')) && before == after
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
