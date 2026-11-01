//! One scanned blob: its keyword hits, their anchor, the listed lines and the declaration a hit names.
use super::scoring::{covering_window, goal_terms, hit_score};
use crate::num::u32_of_usize;
use crate::security::scan::ContentScan;
use octocode_engine::graph::{GraphFactDeclaration, GraphFactsDocument};
use std::path::Path;

/// Bound an optional matchString anchor; hit evidence itself remains complete.
const MAX_DECLARATION_ANCHOR_CHARS: usize = 200;

/// One line holding a keyword.
pub(super) struct Hit {
    /// Its 0-based line index.
    pub(super) index: usize,
    /// The byte of its first keyword in the lowercase line.
    pub(super) at: usize,
    /// It holds every keyword.
    pub(super) every: bool,
    /// How well it fits the goal ([`hit_score`]).
    pub(super) score: (usize, usize),
}

impl Hit {
    /// Its 1-based line number.
    pub(super) fn number(&self) -> u32 {
        u32_of_usize(self.index + 1)
    }
}

/// Where a read anchors: the first and last anchoring lines and the best
/// hit, as line numbers (0: none), and the covering window (hit indexes)
/// when no line holds every keyword.
pub(super) struct Anchor {
    pub(super) first: u32,
    pub(super) last: u32,
    pub(super) best: u32,
}

/// One scanned blob: its lines, as read and lowercased, the private-key
/// blocks no listed line may show, and the declarations its grammar
/// outlines on its keyword lines ([`hit_declarations`]; none for a file
/// type without an outline), sorted by name line.
pub(super) struct Blob<'a> {
    pub(super) raw: Vec<&'a str>,
    pub(super) lowered: Vec<String>,
    pub(super) key_blocks: Vec<(u32, u32)>,
    pub(super) declared: Vec<&'a GraphFactDeclaration>,
    pub(super) path: &'a str,
}

/// The declarations of `outline` whose name line is a `found` keyword line
/// (the only lines a hit is scored or anchored on), by name line, outline
/// order within one line. They borrow the shared parse: the rest of the
/// outline is never copied.
pub(super) fn hit_declarations<'a>(
    outline: Option<&'a GraphFactsDocument>,
    found: &[Found],
) -> Vec<&'a GraphFactDeclaration> {
    let mut declared: Vec<&GraphFactDeclaration> = outline
        .into_iter()
        .flat_map(|outline| &outline.declarations)
        .filter(|declared| {
            found
                .binary_search_by_key(&(name_line(declared) - 1), |&(index, _, _)| index)
                .is_ok()
        })
        .collect();
    declared.sort_by_key(|declared| name_line(declared));
    declared
}

/// A declaration's 1-based name line.
fn name_line(declared: &GraphFactDeclaration) -> usize {
    declared.selection_range.start.line as usize + 1
}

/// A keyword line before scoring: its 0-based index, the byte of its first
/// keyword in the lowercase line and that keyword's length.
pub(super) type Found = (usize, usize, usize);

impl Blob<'_> {
    /// The declaration whose name the keyword at byte `at` of line `index`
    /// is: its name line is the hit line, and the name ends past `at`. A
    /// binary search finds the line's declarations, so scoring every hit
    /// costs no pass over the whole outline.
    pub(super) fn declares(&self, index: usize, at: usize) -> Option<&GraphFactDeclaration> {
        let lower = &self.lowered[index];
        let from = self
            .declared
            .partition_point(|declared| name_line(declared) < index + 1);
        self.declared[from..]
            .iter()
            .copied()
            .take_while(|declared| name_line(declared) == index + 1)
            .find(|declared| {
                let name = declared.name.to_lowercase();
                !name.is_empty()
                    && lower
                        .find(&name)
                        .is_some_and(|start| start + name.len() > at)
            })
    }

    /// Every keyword line, in file order, unscored.
    pub(super) fn found(&self, needles: &[String]) -> Vec<Found> {
        self.lowered
            .iter()
            .enumerate()
            .filter_map(|(index, lower)| {
                let (at, needle) = needles
                    .iter()
                    .filter_map(|needle| lower.find(needle.as_str()).map(|at| (at, needle)))
                    .min()?;
                Some((index, at, needle.len()))
            })
            .collect()
    }

    /// The `found` keyword lines, scored against the goal.
    pub(super) fn hits(&self, found: Vec<Found>, needles: &[String], goal: &str) -> Vec<Hit> {
        let terms = goal_terms(goal, needles, &self.lowered);
        found
            .into_iter()
            .map(|(index, at, length)| Hit {
                index,
                at,
                every: needles
                    .iter()
                    .all(|needle| self.lowered[index].contains(needle.as_str())),
                score: hit_score(
                    &self.lowered,
                    index,
                    at + length,
                    self.declares(index, at).is_some(),
                    &terms,
                ),
            })
            .collect()
    }

    pub(super) fn in_key_block(&self, number: u32) -> bool {
        self.key_blocks
            .iter()
            .any(|&(start, end)| (start..=end).contains(&number))
    }

    /// The read anchors where the listing does: on lines holding every
    /// keyword when any exist; else on the smallest window holding every
    /// keyword (one opened by a declaration first); else on any keyword line.
    pub(super) fn anchor(
        &self,
        hits: &[Hit],
        needles: &[String],
        security: &impl ContentScan,
    ) -> Anchor {
        let any_every = hits.iter().any(|hit| hit.every);
        let window = (!any_every && needles.len() > 1)
            .then(|| {
                covering_window(hits, &self.lowered, needles, |start| {
                    self.declared_head(&hits[start], security).is_some()
                })
            })
            .flatten();
        if let Some((start, end)) = window {
            let first = hits[start].number();
            return Anchor {
                first,
                last: hits[end].number(),
                best: first,
            };
        }
        let (mut first, mut last, mut best) = (0, 0, (0, (0, 0)));
        for hit in hits.iter().filter(|hit| !any_every || hit.every) {
            let number = hit.number();
            if first == 0 || hit.score > best.1 {
                best = (number, hit.score);
            }
            if first == 0 {
                first = number;
            }
            last = number;
        }
        Anchor {
            first,
            last,
            best: best.0,
        }
    }

    /// Every keyword line, sanitized whole before response pagination.
    pub(super) fn listing(&self, hits: &[Hit], security: &impl ContentScan) -> Vec<String> {
        hits.iter()
            .map(|hit| {
                let number = hit.number();
                let shown = if self.in_key_block(number) {
                    crate::security::key_fragment_placeholder()
                } else {
                    security
                        .sanitize(self.raw[hit.index], Path::new(self.path))
                        .map(|(clean, _)| clean)
                        .unwrap_or_else(|_| "[REDACTED]".to_owned())
                };
                format!("{number}{}{shown}", crate::response::numbered::SEPARATOR)
            })
            .collect()
    }

    /// The best hit's declaration line, unless it sits in a private-key block.
    pub(super) fn declaration(
        &self,
        hits: &[Hit],
        best: u32,
        security: &impl ContentScan,
    ) -> Option<String> {
        let hit = hits.iter().find(|hit| hit.number() == best)?;
        if self.in_key_block(best) {
            return None;
        }
        self.declared_head(hit, security)
    }

    /// The hit line as a `matchString` when its keyword is the name of a
    /// block declaration the grammar outlines (a `matchString` + `block`
    /// read then returns the whole declaration), the line is short and
    /// unique in the file, and scanning leaves it unchanged.
    pub(super) fn declared_head(&self, hit: &Hit, security: &impl ContentScan) -> Option<String> {
        self.declares(hit.index, hit.at)
            .filter(|declared| declared.range.end.line > declared.range.start.line)?;
        let head = self.raw[hit.index].trim();
        if head.is_empty() || head.chars().count() > MAX_DECLARATION_ANCHOR_CHARS {
            return None;
        }
        let needle = head.to_lowercase();
        if self
            .lowered
            .iter()
            .filter(|other| other.contains(&needle))
            .count()
            != 1
        {
            return None;
        }
        let (clean, _) = security.sanitize(head, Path::new(self.path)).ok()?;
        (clean == head).then_some(clean)
    }
}
