//! One scanned blob: its keyword hits, their anchor, the listed lines and the declaration a hit names.
use super::scoring::{covering_window, goal_terms, hit_score};
use octocode_engine::graph::GraphFactDeclaration;
use octocode_engine::num::saturating_u32;

/// Bound an optional matchString anchor; hit evidence itself remains complete.
const MAX_DECLARATION_ANCHOR_CHARS: usize = 200;

/// Source lines listed before and after each hit line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::gh_search_code) struct Window {
    pub(in crate::gh_search_code) before: usize,
    pub(in crate::gh_search_code) after: usize,
}

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
        saturating_u32(self.index + 1)
    }
}

/// One scanned blob: its lines, as read and lowercased, the private-key
/// blocks no listed line may show, and the declarations its grammar
/// outlines on its keyword lines ([`hit_declarations`]; none for a file
/// type without an outline), sorted by name line.
pub(super) struct Blob<'a> {
    pub(super) raw: Vec<&'a str>,
    pub(super) lowered: Vec<String>,
    /// Its lines in the shared sealed view
    /// (`source_view::prepare::sealed_view`): judged with the whole blob, so
    /// no listed window shows a value whose secrecy lies outside it. Line N
    /// is source line N.
    pub(super) clean: Vec<&'a str>,
    pub(super) key_blocks: Vec<(u32, u32)>,
    pub(super) declared: Vec<&'a GraphFactDeclaration>,
}

/// The declarations of an outline (`declarations`, in outline order) whose
/// name line is a `found` keyword line (the only lines a hit is scored or
/// anchored on), by name line, outline order within one line. They borrow
/// the outline: the rest of it is never copied.
pub(super) fn hit_declarations<'a>(
    declarations: &'a [GraphFactDeclaration],
    found: &[Found],
) -> Vec<&'a GraphFactDeclaration> {
    let mut declared: Vec<&GraphFactDeclaration> = declarations
        .iter()
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

    /// Every keyword line of the `lowered` lines, in file order, unscored.
    pub(super) fn find(lowered: &[String], needles: &[String]) -> Vec<Found> {
        lowered
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

    /// The hit a cut row's listing centers on, as its line number (0: none):
    /// the best-scoring line holding every keyword when any exist; else the
    /// first hit of the smallest window holding every keyword (one opened by
    /// a declaration first); else the best-scoring keyword line.
    pub(super) fn anchor(&self, hits: &[Hit], needles: &[String]) -> u32 {
        let any_every = hits.iter().any(|hit| hit.every);
        let window = (!any_every && needles.len() > 1)
            .then(|| {
                covering_window(hits, &self.lowered, needles, |start| {
                    self.declared_head(&hits[start]).is_some()
                })
            })
            .flatten();
        if let Some((start, _)) = window {
            return hits[start].number();
        }
        let mut best: Option<(u32, _)> = None;
        for hit in hits.iter().filter(|hit| !any_every || hit.every) {
            if best.is_none_or(|(_, score)| hit.score > score) {
                best = Some((hit.number(), hit.score));
            }
        }
        best.map_or(0, |(number, _)| number)
    }

    /// Every keyword line with its `window` of context, from the sealed view
    /// before response pagination. Overlapping windows merge: a line lists once.
    pub(super) fn listing(&self, hits: &[Hit], window: Window) -> Vec<String> {
        let mut listed = Vec::with_capacity(hits.len());
        // The first line not yet listed: hits ascend, so windows only advance.
        let mut next = 0;
        for hit in hits {
            let from = hit.index.saturating_sub(window.before).max(next);
            let to = hit
                .index
                .saturating_add(window.after)
                .min(self.raw.len().saturating_sub(1));
            for index in from..=to {
                listed.push(self.numbered(index));
            }
            next = next.max(to + 1);
        }
        listed
    }

    /// The placeholders `listed` lines show beyond the source lines they
    /// number: the values this listing redacted.
    pub(super) fn redactions(&self, listed: &[String]) -> usize {
        listed
            .iter()
            .filter_map(|line| {
                let (number, shown) =
                    line.split_once(octocode_runtime_base::response::numbered::SEPARATOR)?;
                let raw = self
                    .raw
                    .get(number.parse::<usize>().ok()?.checked_sub(1)?)?;
                Some(
                    octocode_runtime_base::security::placeholders(shown)
                        .saturating_sub(octocode_runtime_base::security::placeholders(raw)),
                )
            })
            .sum()
    }

    /// One line as `"<number>\t<text>"`: hidden in a private-key block, its
    /// sealed view otherwise.
    fn numbered(&self, index: usize) -> String {
        let number = saturating_u32(index + 1);
        let shown = if self.in_key_block(number) {
            octocode_runtime_base::security::hide_key_line(self.raw[index])
        } else {
            self.clean[index].to_owned()
        };
        format!(
            "{number}{}{shown}",
            octocode_runtime_base::response::numbered::SEPARATOR
        )
    }

    /// The hit line as a `matchString` when its keyword is the name of a
    /// block declaration the grammar outlines (a `matchString` + `block`
    /// read then returns the whole declaration), the line is short and
    /// unique in the file, and the sealed view leaves it unchanged.
    pub(super) fn declared_head(&self, hit: &Hit) -> Option<String> {
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
        (self.clean[hit.index] == self.raw[hit.index]).then(|| head.to_owned())
    }
}
