//! `find`: ranked node search over names and keys.
//!
//! Three tiers, strongest first: the whole text equals a key or name
//! (`exact`); its identifier words start words of a name or key (`words`,
//! stated `prefix` when the name also starts with the whole text); the whole
//! text sits inside a name or key (`prefix`, `contains`, `path`). Text and names split into
//! identifier words at case changes and at every separator (`snapshot
//! reader` finds `SnapshotReader`, `parse_config` finds `parseConfig`). Word
//! matches keep the nodes that match the most query words, counted over
//! names and keys and over names alone, so words of a file path never
//! outvote a name; more matched words rank first, and a row that misses a
//! query word names the words it matched.
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
    /// The `match` a row states: its tier, or `prefix` for a word match
    /// whose name starts with the whole text.
    label: Tier,
    /// Query words a word match matches; more rank first.
    count: usize,
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
    /// The name's words and runs.
    name_parts: Vec<String>,
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
        let mut name_parts = name_words.clone();
        name_parts.extend(runs(name));
        let mut words = name_parts.clone();
        words.extend(identifier_words(key));
        words.extend(runs(key));
        Self {
            name_words,
            name_parts,
            words,
        }
    }

    /// A query word hits when it starts one of this node's words.
    fn hits(&self, word: &str) -> bool {
        self.words.iter().any(|part| part.starts_with(word))
    }

    /// A query word hits the name when it starts one of the name's words.
    fn hits_name(&self, word: &str) -> bool {
        self.name_parts.iter().any(|part| part.starts_with(word))
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
    g.load_degrees(cancel)?;
    let mut found = Vec::new();
    for id in g.t.node_ids() {
        check(cancel)?;
        if kind.is_some_and(|kind| g.t.nodes[id as usize].kind != kind) {
            continue;
        }
        let name = g.t.str(g.t.nodes[id as usize].name).to_lowercase();
        let key = g.key(id).to_lowercase();
        if name == lower || key == lower {
            let all = words.len();
            found.push((id, Some(Tier::Exact), (all, all), u32::from(key == lower)));
            continue;
        }
        let substring = words
            .iter()
            .any(|word| name.contains(word.as_str()) || key.contains(word.as_str()));
        // Query words the node matches: over its name and key, and over its
        // name alone.
        let count = if substring {
            let terms = Terms::new(g, id);
            let hits = |hit: &dyn Fn(&str) -> bool| words.iter().filter(|w| hit(w)).count();
            (hits(&|w| terms.hits(w)), hits(&|w| terms.hits_name(w)))
        } else {
            (0, 0)
        };
        let whole = if name.starts_with(&lower) {
            Some(Tier::Prefix)
        } else if name.contains(&lower) {
            Some(Tier::Contains)
        } else {
            key.contains(&lower).then_some(Tier::Path)
        };
        if count.0 > 0 || whole.is_some() {
            found.push((id, whole, count, 0));
        }
    }
    let quorum = |of: fn(&(usize, usize)) -> usize| {
        found
            .iter()
            .map(|(_, _, count, _)| of(count))
            .max()
            .unwrap_or(0)
    };
    let (overall, by_name) = (quorum(|count| count.0), quorum(|count| count.1));
    let mut generic = HashMap::new();
    let mut hits = Vec::new();
    for (id, whole, (count, name_count), exact_key) in found {
        check(cancel)?;
        let in_quorum = (overall > 0 && count == overall) || (by_name > 0 && name_count == by_name);
        let tier = match whole {
            Some(Tier::Exact) => Tier::Exact,
            _ if in_quorum => Tier::Words,
            Some(tier) => tier,
            None => continue,
        };
        let label = match whole {
            Some(Tier::Prefix) => Tier::Prefix,
            _ => tier,
        };
        let words_tier = tier == Tier::Words;
        let (strength, unmatched, matched) = if words_tier {
            let terms = Terms::new(g, id);
            let (strength, unmatched) = terms.strength(&words);
            let matched = (count < words.len())
                .then(|| words.iter().filter(|w| terms.hits(w)).cloned().collect());
            (strength, unmatched, matched)
        } else {
            (exact_key, 0, None)
        };
        hits.push(Hit {
            id,
            tier,
            label,
            count: if words_tier { count } else { 0 },
            strength,
            unmatched,
            centrality: g.centrality(id, &mut generic),
            matched,
        });
    }
    hits.sort_by(|a, b| {
        a.tier
            .cmp(&b.tier)
            .then(b.count.cmp(&a.count))
            .then(b.strength.cmp(&a.strength))
            .then(a.unmatched.cmp(&b.unmatched))
            .then(b.centrality.total_cmp(&a.centrality))
            .then_with(|| g.key(a.id).cmp(g.key(b.id)))
    });
    Ok(hits)
}

/// Whether `hit`, a ranked match of `text`, is a near name of it, one that may
/// stand in for a reference no node has: it matches the whole text (as
/// written, or with one typo), or the query words its name spells, each as
/// written, as a prefix or with one typo, hold more of the text's characters
/// than the words it leaves out. A node that spells less shares a word, not
/// the name: the `find` lead lists it.
pub(super) fn is_near(g: &Graph, text: &str, hit: &Hit) -> bool {
    if hit.tier != Tier::Words {
        return true;
    }
    let terms = Terms::new(g, hit.id);
    let compact = |text: &str| {
        text.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let name = g.t.str(g.t.nodes[hit.id as usize].name);
    if one_typo(&compact(text), &compact(name)) {
        return true;
    }
    let (mut spelled, mut left) = (0, 0);
    for word in identifier_words(text) {
        let size = word.chars().count();
        if terms
            .name_parts
            .iter()
            .any(|part| part.starts_with(word.as_str()) || one_typo(&word, part))
        {
            spelled += size;
        } else {
            left += size;
        }
    }
    spelled > left
}

/// Whether one edit (a character inserted, removed or replaced, or two
/// neighbors swapped) turns `a` into `b`, leaving more of the shorter in
/// place than it changes: two letters one edit apart are different words.
fn one_typo(a: &str, b: &str) -> bool {
    let (a, b) = (a.chars().collect::<Vec<_>>(), b.chars().collect::<Vec<_>>());
    let (short, long) = if a.len() <= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    let prefix = short
        .iter()
        .zip(long.iter())
        .take_while(|(x, y)| x == y)
        .count();
    if long.len() == short.len() + 1 {
        // Insertion: the rest matches one character on.
        return short[prefix..] == long[prefix + 1..] && short.len() > 1;
    }
    if long.len() != short.len() || prefix == short.len() {
        return false;
    }
    if short[prefix + 1..] == long[prefix + 1..] {
        // Replacement.
        return short.len() - 1 > 1;
    }
    // Swap of two neighbors.
    prefix + 1 < short.len()
        && short[prefix] == long[prefix + 1]
        && short[prefix + 1] == long[prefix]
        && short[prefix + 2..] == long[prefix + 2..]
        && short.len() - 2 > 2
}

impl Graph {
    /// Ranking counts incoming edges: tables decoded without an edge index
    /// take the counts from the snapshot's lookup section instead.
    fn load_degrees(
        &self,
        cancel: &dyn crate::execution::cancel::CancellationCheck,
    ) -> Result<(), GraphOutput> {
        let Some(reader) = &self.selective else {
            return Ok(());
        };
        if self.t.in_offsets.len() == self.t.nodes.len() + 1 || reader.borrow().has_degrees() {
            return Ok(());
        }
        reader
            .borrow_mut()
            .load_degrees(&self.t, cancel)
            .map_err(|message| *decode_failure(message, cancel))
    }

    /// `ln(1 + incoming calls and imports)`, damped for a generic name
    /// (declared in more than [`GENERIC_NAME_FILES`] files) or a `_` prefix.
    fn centrality(&self, id: u32, generic: &mut HashMap<String, bool>) -> f64 {
        let counts = match &self.selective {
            Some(reader) if reader.borrow().has_degrees() => reader.borrow().degrees(id, true),
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
    let mut window = Window::budgeted(range, g.row_budget());
    for hit in ranked(g, text, kind, cancel)? {
        window.try_push(|| {
            let mut row = g.node_json(hit.id, cancel)?;
            row.insert("match".into(), json!(hit.label.as_str()));
            if let Some(matched) = hit.matched {
                row.insert("matchedWords".into(), json!(matched));
            }
            Ok(Value::Object(row))
        })?;
    }
    Ok(window.finish())
}
