//! `find`: ranked node search over names and keys.
//!
//! Three tiers, strongest first: the whole text equals a key or name
//! (`exact`); its identifier words start words of a name or key (`words`);
//! the whole text sits inside a name or key (`prefix`, `contains`, `path`).
//! Text and names split into identifier words at case changes and at every
//! separator (`snapshot reader` finds `SnapshotReader`, `parse_config` finds
//! `parseConfig`). Word matches keep the nodes that match the most query
//! words: every word is required whenever some node has them all, and
//! otherwise rows name the words they matched.
use super::*;
use octocode_engine::text::identifier_words;
use std::collections::HashMap;

/// A name declared in more files than this is generic and ranks lower.
const GENERIC_NAME_FILES: usize = 5;
/// The centrality weight left to generic and `_`-prefixed names.
const DAMPING: f64 = 0.1;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Exact,
    Words,
    Prefix,
    Contains,
    Path,
}

impl Tier {
    fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Words => "words",
            Self::Prefix => "prefix",
            Self::Contains => "contains",
            Self::Path => "path",
        }
    }
}

pub(super) struct Hit {
    pub(super) id: u32,
    tier: Tier,
    strength: u32,
    unmatched: usize,
    centrality: f64,
    /// Query words this node matches; set only when no node matches them all.
    matched: Option<Vec<String>>,
}

/// One node's name words, and every word of its name and key plus each
/// whole alphanumeric run (`snapshotreader`), so joined spellings match too.
struct Terms {
    name_words: Vec<String>,
    words: Vec<String>,
}

impl Terms {
    fn new(g: &Graph, id: u32) -> Self {
        let (name, key) = (g.t.str(g.t.nodes[id as usize].name), g.key(id));
        let runs = |text: &str| {
            text.split(|c: char| !c.is_alphanumeric())
                .filter(|run| !run.is_empty())
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
        };
        let name_words = identifier_words(name);
        let mut words = name_words.clone();
        words.extend(runs(name));
        words.extend(identifier_words(key));
        words.extend(runs(key));
        Self { name_words, words }
    }

    /// A query word hits when it starts one of this node's words.
    fn hits(&self, word: &str) -> bool {
        self.words.iter().any(|part| part.starts_with(word))
    }

    /// Each query word scores its strongest hit: a name word equal (6), a
    /// name word it starts (5), then a key word equal (3) or started (2).
    /// Also counts the name words no query word starts.
    fn strength(&self, words: &[String]) -> (u32, usize) {
        let level = |word: &str, parts: &[String]| {
            if parts.iter().any(|part| part == word) {
                3
            } else {
                2 * u32::from(parts.iter().any(|part| part.starts_with(word)))
            }
        };
        let total = words
            .iter()
            .map(|word| match level(word, &self.name_words) {
                0 => level(word, &self.words),
                hit => hit + 3,
            })
            .sum();
        let unmatched = self
            .name_words
            .iter()
            .filter(|part| !words.iter().any(|word| part.starts_with(word.as_str())))
            .count();
        (total, unmatched)
    }
}

/// Every matching node in rank order: tier, word strength, fewest unmatched
/// name words, damped incoming calls and imports, then key.
pub(super) fn ranked(
    g: &Graph,
    text: &str,
    kind: Option<NodeKind>,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<Vec<Hit>, GraphOutput> {
    let lower = text.to_lowercase();
    let mut words = identifier_words(text);
    let mut seen = std::collections::BTreeSet::new();
    words.retain(|word| seen.insert(word.clone()));
    let mut found = Vec::new();
    for id in 0..g.t.nodes.len() as u32 {
        check(cancel)?;
        if kind.is_some_and(|kind| g.t.nodes[id as usize].kind != kind) {
            continue;
        }
        let name = g.t.str(g.t.nodes[id as usize].name).to_lowercase();
        let key = g.key(id).to_lowercase();
        if name == lower || key == lower {
            found.push((id, Some(Tier::Exact), words.len(), u32::from(key == lower)));
            continue;
        }
        let substring = words
            .iter()
            .any(|word| name.contains(word.as_str()) || key.contains(word.as_str()));
        let count = if substring {
            let terms = Terms::new(g, id);
            words.iter().filter(|word| terms.hits(word)).count()
        } else {
            0
        };
        let whole = if name.starts_with(&lower) {
            Some(Tier::Prefix)
        } else if name.contains(&lower) {
            Some(Tier::Contains)
        } else {
            key.contains(&lower).then_some(Tier::Path)
        };
        if count > 0 || whole.is_some() {
            found.push((id, whole, count, 0));
        }
    }
    let quorum = found
        .iter()
        .map(|(_, _, count, _)| *count)
        .max()
        .unwrap_or(0);
    let mut generic = HashMap::new();
    let mut hits = Vec::new();
    for (id, whole, count, exact_key) in found {
        check(cancel)?;
        let tier = match whole {
            Some(Tier::Exact) => Tier::Exact,
            _ if quorum > 0 && count == quorum => Tier::Words,
            Some(tier) => tier,
            None => continue,
        };
        let (strength, unmatched, matched) = if tier == Tier::Words {
            let terms = Terms::new(g, id);
            let (strength, unmatched) = terms.strength(&words);
            let matched = (quorum < words.len())
                .then(|| words.iter().filter(|w| terms.hits(w)).cloned().collect());
            (strength, unmatched, matched)
        } else {
            (exact_key, 0, None)
        };
        hits.push(Hit {
            id,
            tier,
            strength,
            unmatched,
            centrality: g.centrality(id, &mut generic),
            matched,
        });
    }
    hits.sort_by(|a, b| {
        a.tier
            .cmp(&b.tier)
            .then(b.strength.cmp(&a.strength))
            .then(a.unmatched.cmp(&b.unmatched))
            .then(b.centrality.total_cmp(&a.centrality))
            .then_with(|| g.key(a.id).cmp(g.key(b.id)))
    });
    Ok(hits)
}

impl Graph {
    /// `ln(1 + incoming calls and imports)`, damped for a generic name
    /// (declared in more than [`GENERIC_NAME_FILES`] files) or a `_` prefix.
    fn centrality(&self, id: u32, generic: &mut HashMap<String, bool>) -> f64 {
        let counts = match &self.selective {
            Some(reader) if reader.borrow().is_point() => reader.borrow().degrees(id, true),
            _ => degree_counts(self.t.incoming(id)),
        };
        let degree = [EdgeKind::Calls, EdgeKind::Imports]
            .iter()
            .filter_map(|kind| counts.get(kind.as_str()))
            .sum::<u64>();
        let name = self.t.str(self.t.nodes[id as usize].name);
        let lower = name.to_lowercase();
        let common = *generic.entry(lower.clone()).or_insert_with(|| {
            let mut files = std::collections::BTreeSet::new();
            self.name_range(&lower).any(|other| {
                files.insert(self.t.nodes[other as usize].file) && files.len() > GENERIC_NAME_FILES
            })
        });
        let weight = if common || name.starts_with('_') {
            DAMPING
        } else {
            1.0
        };
        (degree as f64).ln_1p() * weight
    }
}

pub(super) fn find(
    g: &Graph,
    text: &str,
    kind: Option<NodeKind>,
    range: std::ops::Range<usize>,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<Page, GraphOutput> {
    let mut window = Window::new(range);
    for hit in ranked(g, text, kind, cancel)? {
        window.try_push(|| {
            let mut row = g.node_json(hit.id, cancel)?;
            row.insert("match".into(), json!(hit.tier.as_str()));
            if let Some(matched) = hit.matched {
                row.insert("matchedWords".into(), json!(matched));
            }
            Ok(Value::Object(row))
        })?;
    }
    Ok(window.finish())
}
