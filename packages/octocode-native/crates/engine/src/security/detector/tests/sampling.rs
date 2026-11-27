//! Texts generated from a pattern's own syntax tree.

use regex_syntax::hir::{Class, Hir, HirKind};

pub(super) struct Lcg(pub(super) u64);
impl Lcg {
    pub(super) fn below(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % bound.max(1)
    }
}
pub(super) fn sample(hir: &Hir, rng: &mut Lcg, out: &mut String) {
    match hir.kind() {
        HirKind::Literal(literal) => out.push_str(&String::from_utf8_lossy(&literal.0)),
        HirKind::Class(Class::Unicode(class)) => {
            let ranges = class.ranges();
            let size: u32 = ranges
                .iter()
                .map(|range| range.end() as u32 - range.start() as u32 + 1)
                .sum();
            let ascii: Vec<_> = ranges
                .iter()
                .filter(|range| range.start().is_ascii())
                .collect();
            // A small class mixing ASCII and other characters is a case
            // fold (`[Kk\u{212A}]`, `[Ssſ]`): any member, so Unicode case
            // variants are sampled. A large class: ASCII members when it
            // has any, so samples stay readable.
            let fold = size <= 8 && !ascii.is_empty() && ascii.len() < ranges.len();
            let range = if fold || ascii.is_empty() {
                ranges[rng.below(ranges.len())]
            } else {
                *ascii[rng.below(ascii.len())]
            };
            let end = if range.start().is_ascii() && !fold {
                range.end().min('\x7f')
            } else {
                range.end()
            };
            let span = end as usize - range.start() as usize + 1;
            let pick = range.start() as u32 + rng.below(span.min(64)) as u32;
            out.push(char::from_u32(pick).unwrap_or(range.start()));
        }
        HirKind::Class(Class::Bytes(class)) => {
            let range = class.ranges()[rng.below(class.ranges().len())];
            out.push(char::from(range.start()));
        }
        HirKind::Repetition(repetition) => {
            let spread = repetition
                .max
                .map_or(3, |max| (max - repetition.min).min(3));
            for _ in 0..repetition.min as usize + rng.below(spread as usize + 1) {
                sample(&repetition.sub, rng, out);
            }
        }
        HirKind::Capture(capture) => sample(&capture.sub, rng, out),
        HirKind::Concat(parts) => parts.iter().for_each(|part| sample(part, rng, out)),
        HirKind::Alternation(branches) => {
            sample(&branches[rng.below(branches.len())], rng, out);
        }
        HirKind::Empty | HirKind::Look(_) => {}
    }
}
