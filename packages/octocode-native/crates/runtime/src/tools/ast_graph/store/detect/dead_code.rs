//! Dead-code tiering: the text-mention scan that corroborates findings.
use super::*;

/// Directories the text-mention scan skips on top of the shared
/// syntax-visible prune (build output, deps, VCS, credential stores):
/// vendored and virtualenv copies would mention every name.
pub(super) const MENTION_EXTRA_SKIP_DIRS: &[&str] = &["venv", "vendor"];
pub(super) const MENTION_MAX_FILE: u64 = 2 << 20;

pub(super) const MENTION_MAX_TOTAL: u64 = 512 << 20;

/// Markup and single-file components whose script blocks import modules
/// (`<script>import { run } from './cmd'</script>`, an MDX `import`): no
/// grammar the engine parses covers them, yet a name in them may load an
/// export, as one in source code does.
pub(super) const SCRIPT_BEARING_MARKUP: &[&str] = &["vue", "svelte", "astro", "mdx", "html", "htm"];

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
/// path-like tokens (`'./worker.ts'`, `"src/tool.ts"`) and which export names
/// appear as identifiers, and where: every file, with its first line. This is
/// the SCARF/vulture "is it mentioned anywhere" safety check. A name in a
/// source file (any grammar the engine parses, or [`SCRIPT_BEARING_MARKUP`])
/// or qualified by a module path or object (`mod.run`, `index.run`) is a
/// reference; a bare word of other text (a README's "run") is only a word.
pub(super) fn scan_mentions(
    root: &std::path::Path,
    stems: &BTreeSet<String>,
    names: &BTreeSet<String>,
) -> (MentionHits, bool) {
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
    let walker = crate::tools::pruned_walk(
        root,
        MENTION_EXTRA_SKIP_DIRS
            .iter()
            .map(|name| (*name).to_owned()),
    )
    .build_parallel();
    walker.run(|| {
        let (stem_set, name_set) = (&stem_set, &name_set);
        let spent = &spent;
        let truncated = &truncated;
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
            let code = SCRIPT_BEARING_MARKUP.contains(&extension.as_str())
                || octocode_engine::signatures::languages::find_entry(&extension).is_some();
            let text = decoded.text;
            let mut local: [BTreeMap<&str, usize>; 3] = Default::default();
            for (index, line) in text.lines().enumerate() {
                for token in line.split(|c: char| !(c.is_alphanumeric() || "_$./@-".contains(c))) {
                    if token.is_empty() {
                        continue;
                    }
                    if token.contains(['/', '.']) {
                        let last = token.rsplit('/').next().unwrap_or(token);
                        let stem = last.split('.').next().unwrap_or(last);
                        if let Some(&stem) = stem_set.get(stem) {
                            local[0].entry(stem).or_insert(index + 1);
                        }
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
        truncated.load(Ordering::Relaxed),
    )
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

pub(super) fn file_stem(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.split('.').next().unwrap_or(name).to_owned()
}

/// Re-tiers dead-code findings with the mention scan.
pub(super) fn tier_dead_code(
    findings: &mut [Finding],
    root: &std::path::Path,
    notes: &mut Vec<Value>,
) {
    let mut stems = BTreeSet::new();
    let mut names = BTreeSet::new();
    for finding in findings.iter() {
        match finding.detector {
            "unreachable-file" => {
                stems.insert(file_stem(&finding.subject));
            }
            "unused-export" => {
                // Every name, short ones too: tier 90 claims no code or
                // qualified reference, so no name may skip the check.
                for name in finding.evidence["exports"].as_array().into_iter().flatten() {
                    if let Some(name) = name.as_str() {
                        names.insert(name.to_owned());
                    }
                }
            }
            _ => {}
        }
    }
    if stems.is_empty() && names.is_empty() {
        return;
    }
    let (hits, truncated) = scan_mentions(root, &stems, &names);
    if truncated {
        notes.push(json!({"detector": "unreachable-file", "skipped": "mention scan stopped at 512 MB of text; later files unchecked"}));
    }
    for finding in findings.iter_mut() {
        match finding.detector {
            "unreachable-file" => {
                let mentions =
                    mentions_outside(&hits.stems, &file_stem(&finding.subject), &finding.subject);
                if !mentions.is_empty() {
                    finding.tier = 60;
                    finding.evidence["mentionedIn"] = json!(mentions);
                    finding.controls.push(
                        "its path is mentioned in text (string, config, or script): likely loaded dynamically",
                    );
                } else if finding.evidence["orphan"] == true {
                    finding.tier = 100;
                }
            }
            "unused-export" => {
                let exports = finding.evidence["exports"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let mut referenced = BTreeMap::new();
                let mut words = BTreeMap::new();
                for name in exports.iter().filter_map(Value::as_str) {
                    for (hits, kept) in [
                        (&hits.references, &mut referenced),
                        (&hits.words, &mut words),
                    ] {
                        let mentions = mentions_outside(hits, name, &finding.subject);
                        if !mentions.is_empty() {
                            kept.insert(name.to_owned(), mentions);
                        }
                    }
                }
                if !words.is_empty() {
                    finding.evidence["textMentions"] = json!(words);
                }
                if !referenced.is_empty() {
                    if referenced.len() == exports.len() {
                        finding.tier = 60;
                    }
                    finding.evidence["mentionedIn"] = json!(referenced);
                }
            }
            _ => {}
        }
    }
}

/// Dedupe by subject: the strongest finding leads, others corroborate it.
pub(super) fn corroborate(mut findings: Vec<Finding>) -> Vec<Finding> {
    findings.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    let mut merged: Vec<Finding> = Vec::new();
    let mut lead = BTreeMap::<String, usize>::new();
    for finding in findings {
        match lead.get(&finding.subject) {
            Some(&at) => {
                let leader = &mut merged[at];
                if leader.detector != finding.detector
                    && !leader.corroborated_by.contains(&finding.detector)
                {
                    leader.corroborated_by.push(finding.detector);
                    leader.score = (leader.score * 1.1).min(1.0);
                }
            }
            None => {
                lead.insert(finding.subject.clone(), merged.len());
            }
        }
        merged.push(finding);
    }
    merged.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    merged
}
