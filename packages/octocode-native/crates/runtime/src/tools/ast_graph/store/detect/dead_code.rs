//! Dead-code tiering: re-tiers findings by the mentions the text scan
//! ([`super::mentions`]) found, and spreads liveness from mentioned files.
use super::*;

pub(super) fn file_stem(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.split('.').next().unwrap_or(name).to_owned()
}

/// The stem keys a text mention of file `subject` may hit: its bare stem (a
/// path naming no directory, a whole string literal) and `dir/stem` (a path
/// naming the directory it is in; `root` names a file at the root).
fn subject_keys(subject: &str, root: &str) -> [String; 2] {
    let stem = file_stem(subject);
    let dir = subject
        .rsplit_once('/')
        .map_or(root, |(head, _)| head.rsplit('/').next().unwrap_or(head));
    [path_stem_key(dir, &stem), stem]
}

/// The file of a `file:line` mention.
fn mention_file(mention: &str) -> &str {
    mention.rsplit_once(':').map_or(mention, |(file, _)| file)
}

/// Whether the graph explains a `file:line` mention of `subject`: the
/// mentioning file imports `subject` (the mention is the import the graph
/// already followed), or, for `test-only`, it is a test or support file,
/// which is how such a file is reached.
fn explained(v: &View, subject: &str, mention: &str, test_only: bool) -> bool {
    let (Some(from), Some(to)) = (v.t.by_key(mention_file(mention)), v.t.by_key(subject)) else {
        return false;
    };
    (test_only && v.flags(from) & (ROLE_TEST | ROLE_SUPPORT) != 0)
        || v.t
            .out(from)
            .iter()
            .any(|edge| edge.kind == EdgeKind::Imports && edge.dst == to)
}

/// `file:line` mentions merged from several key lists: per file its first
/// line, files in path order.
fn merged(lists: impl IntoIterator<Item = Vec<String>>) -> Vec<String> {
    let mut first = BTreeMap::<String, (usize, String)>::new();
    for mention in lists.into_iter().flatten() {
        let line = mention
            .rsplit_once(':')
            .and_then(|(_, line)| line.parse().ok())
            .unwrap_or(usize::MAX);
        let file = mention_file(&mention).to_owned();
        match first.get(&file) {
            Some((seen, _)) if *seen <= line => {}
            _ => {
                first.insert(file, (line, mention));
            }
        }
    }
    first.into_values().map(|(_, mention)| mention).collect()
}

/// Re-tiers a file finding by the text that names it: a mentioned file is
/// likely loaded by name (tier 60, `mentionedIn`). Mentions the graph
/// explains ([`explained`]) are no such evidence. An unmentioned orphan is
/// 100 when every text file was read.
fn tier_mentioned_file(v: &View, hits: &MentionHits, root: &str, finding: &mut Finding) {
    let test_only = finding.detector == "test-only";
    let mentions = merged(
        subject_keys(&finding.subject, root)
            .iter()
            .map(|key| mentions_outside(&hits.stems, key, &finding.subject)),
    )
    .into_iter()
    .filter(|mention| !explained(v, &finding.subject, mention, test_only))
    .collect::<Vec<_>>();
    if !mentions.is_empty() {
        finding.tier = 60;
        finding.evidence["mentionedIn"] = json!(mentions);
        finding.controls.push(
            "its path is mentioned in text (string, config, or script): likely loaded dynamically",
        );
    } else if finding.evidence["orphan"] == true && hits.unread.is_empty() {
        finding.tier = 100;
    }
}

/// Per export name, the files that declare a symbol of that name: a bare
/// name there is that file's own binding.
fn declaring_files<'a>(
    v: &View<'a>,
    names: &BTreeSet<String>,
) -> BTreeMap<&'a str, BTreeSet<&'a str>> {
    let mut declared = BTreeMap::<&str, BTreeSet<&str>>::new();
    for node in &v.t.nodes {
        if node.kind != NodeKind::Symbol || node.file == NONE {
            continue;
        }
        let name = v.t.str(node.name);
        if names.contains(name) {
            declared.entry(name).or_default().insert(v.key(node.file));
        }
    }
    declared
}

/// Re-tiers dead-code findings with the mention scan; what proves that scan
/// current, `None` when no proof could be taken. Files the scan could not
/// read are named in a note: what they mention is unknown.
pub(super) fn tier_dead_code(
    v: &View,
    findings: &mut Vec<Finding>,
    notes: &mut Vec<Value>,
    root: &std::path::Path,
    global: octocode_engine::search::walk::GlobalIgnore,
    stopped: &(dyn Fn() -> bool + Sync),
) -> Option<octocode_engine::stamp::Proof> {
    let root_name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut stems = BTreeSet::new();
    let mut names = BTreeSet::new();
    for finding in findings.iter() {
        match finding.detector {
            "unreachable-file" | "test-only" => {
                stems.extend(subject_keys(&finding.subject, &root_name));
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
        return Some(octocode_engine::stamp::Proof::default());
    }
    // A stopped scan decides nothing: the request ends with it.
    let hits = memoized_mentions(root, global, &stems, &names, stopped)?;
    if !hits.unread.is_empty() {
        notes.push(json!({
            "detectors": ["unreachable-file", "test-only", "unused-export"],
            "skipped": format!(
                "{} text file(s) could not be read: what they mention is unknown, so no file is tier 100",
                hits.unread.len()
            ),
            "unread": hits.unread,
        }));
    }
    let declared = declaring_files(v, &names);
    let mut split = Vec::new();
    for finding in findings.iter_mut() {
        match finding.detector {
            "unreachable-file" | "test-only" => {
                tier_mentioned_file(v, &hits, &root_name, finding);
            }
            "unused-export" => {
                let references = |name: &str| {
                    let own = declared.get(name);
                    let bare = mentions_outside(&hits.bare, name, &finding.subject)
                        .into_iter()
                        .filter(|mention| {
                            own.is_none_or(|files| !files.contains(mention_file(mention)))
                        })
                        .collect();
                    merged([
                        mentions_outside(&hits.qualified, name, &finding.subject),
                        bare,
                    ])
                };
                let exports = finding.evidence["exports"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                let mut referenced = BTreeMap::new();
                let mut words = BTreeMap::new();
                for name in &exports {
                    for (mentions, kept) in [
                        (references(name), &mut referenced),
                        (
                            mentions_outside(&hits.words, name, &finding.subject),
                            &mut words,
                        ),
                    ] {
                        if !mentions.is_empty() {
                            kept.insert(name.clone(), mentions);
                        }
                    }
                }
                if referenced.is_empty() || referenced.len() == exports.len() || finding.tier <= 60
                {
                    if !referenced.is_empty() && referenced.len() == exports.len() {
                        finding.tier = 60;
                    }
                    mention_evidence(finding, referenced, words);
                } else {
                    // Tier 90 claims no code reference for every name it
                    // lists: the referenced names are a finding of their own.
                    let (named, unnamed): (Vec<_>, Vec<_>) = exports
                        .iter()
                        .partition(|name| referenced.contains_key(*name));
                    let (mut words_named, words_unnamed): (BTreeMap<_, _>, BTreeMap<_, _>) = words
                        .into_iter()
                        .partition(|(name, _)| referenced.contains_key(name));
                    if let Some(mut referenced_part) = regroup_unused_export(v, finding, &named) {
                        referenced_part.tier = 60;
                        mention_evidence(
                            &mut referenced_part,
                            referenced,
                            std::mem::take(&mut words_named),
                        );
                        if let Some(rest) = regroup_unused_export(v, finding, &unnamed) {
                            *finding = rest;
                            mention_evidence(finding, BTreeMap::new(), words_unnamed);
                        }
                        split.push(referenced_part);
                    }
                }
            }
            _ => {}
        }
    }
    findings.extend(split);
    hits.proof.clone()
}

/// Writes an `unused-export` finding's mentions: code references
/// (`mentionedIn`) and prose words (`textMentions`).
fn mention_evidence(
    finding: &mut Finding,
    referenced: BTreeMap<String, Vec<String>>,
    words: BTreeMap<String, Vec<String>>,
) {
    if !words.is_empty() {
        finding.evidence["textMentions"] = json!(words);
    }
    if !referenced.is_empty() {
        finding.evidence["mentionedIn"] = json!(referenced);
    }
}

/// A mentioned unreachable or test-only file (a bundler entry or a loader
/// names it) is as likely live as its mention, and so is every file it
/// imports: their `unreachable-file` and `test-only` findings drop to tier 60
/// and name it in `evidence.reachedFrom`. Run after [`tier_dead_code`].
pub(super) fn spread_mentioned_liveness(v: &View, findings: &mut [Finding]) {
    let seeds = findings
        .iter()
        .filter(|f| {
            matches!(f.detector, "unreachable-file" | "test-only")
                && f.evidence.get("mentionedIn").is_some()
        })
        .filter_map(|f| v.t.by_key(&f.subject))
        .collect::<Vec<_>>();
    if seeds.is_empty() {
        return;
    }
    let origin = v.origins(&seeds, false);
    for finding in findings.iter_mut() {
        if !matches!(finding.detector, "unreachable-file" | "test-only")
            || finding.evidence.get("mentionedIn").is_some()
        {
            continue;
        }
        let Some(file) = v.t.by_key(&finding.subject) else {
            continue;
        };
        let from = origin[file as usize];
        if from == NONE || from == file {
            continue;
        }
        finding.tier = finding.tier.min(60);
        finding.evidence["reachedFrom"] = json!(v.key(from));
        finding
            .controls
            .push("a file mentioned in text imports it: live if that file is loaded by name");
    }
}

/// Corroborates by subject: the strongest finding on a subject leads, and
/// each other detector flagging that subject is recorded on the leader and
/// raises its score. Every finding stays listed, so `byDetector` counts
/// each detector's findings.
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
