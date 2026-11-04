//! Dead-code tiering: re-tiers findings by the mentions the text scan
//! ([`super::mentions`]) found, and spreads liveness from mentioned files.
use super::*;

pub(super) fn file_stem(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.split('.').next().unwrap_or(name).to_owned()
}

/// Whether the graph explains a `file:line` mention of `subject`: the
/// mentioning file is a test or imports `subject`, so it names it to load it.
fn explained(v: &View, subject: &str, mention: &str) -> bool {
    let file = mention.rsplit_once(':').map_or(mention, |(file, _)| file);
    let (Some(from), Some(to)) = (v.t.by_key(file), v.t.by_key(subject)) else {
        return false;
    };
    v.flags(from) & ROLE_TEST != 0 || v.inc[to as usize].binary_search(&from).is_ok()
}

/// Re-tiers a file finding by the text that names it: a mentioned file is
/// likely loaded by name (tier 60, `mentionedIn`); a `test-only` file counts
/// only mentions the graph does not explain. An unmentioned orphan is 100.
fn tier_mentioned_file(v: &View, stems: &Mentions, finding: &mut Finding) {
    let test_only = finding.detector == "test-only";
    let mentions = mentions_outside(stems, &file_stem(&finding.subject), &finding.subject)
        .into_iter()
        .filter(|mention| !(test_only && explained(v, &finding.subject, mention)))
        .collect::<Vec<_>>();
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

/// Re-tiers dead-code findings with the mention scan.
pub(super) fn tier_dead_code(
    v: &View,
    findings: &mut [Finding],
    root: &std::path::Path,
    notes: &mut Vec<Value>,
) {
    let mut stems = BTreeSet::new();
    let mut names = BTreeSet::new();
    for finding in findings.iter() {
        match finding.detector {
            "unreachable-file" | "test-only" => {
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
    let (hits, gaps) = scan_mentions(root, &stems, &names);
    if gaps.truncated {
        notes.push(json!({"detector": "unreachable-file", "skipped": "mention scan stopped at 512 MB of text; later files unchecked"}));
    }
    if gaps.oversized > 0 {
        notes.push(json!({"detector": "unreachable-file", "skipped": format!("mention scan skipped {} over 2 MiB; a mention there is not seen", octocode_engine::text::counted(gaps.oversized, "file", "files"))}));
    }
    for finding in findings.iter_mut() {
        match finding.detector {
            "unreachable-file" | "test-only" => tier_mentioned_file(v, &hits.stems, finding),
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
