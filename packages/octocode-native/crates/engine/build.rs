//! Build script: derives the secret detector's prescan gate from the pattern
//! table, so no process parses about 300 pattern regexes at startup.
//!
//! For each pattern in `src/security/patterns.rs` (joined from its slices in
//! `src/security/patterns/`) it extracts a gate with regex-syntax: a finite
//! set of literals (each at least 3 bytes, at most 64 of them) one of which
//! occurs in every match. The *required prefix literals* come first; a
//! pattern without them gates on literals its structure requires inside the
//! match (a part of a concatenation). Patterns are parsed with their flags, so
//! a `(?i)` pattern's literals include every Unicode case variant the regex
//! matches (`ſ` for `s`, `K` (U+212A) for `k`). The runtime builds one
//! ASCII-case-insensitive aho-corasick automaton over the lowercased literals
//! written to `OUT_DIR/secret_gates.rs`, so a pattern neither finds a gate for
//! is read once more with its ASCII capitals folded and runs of parts counted:
//! a `(?i)` label word the parser split into one part per letter then gives
//! one literal per Unicode case variant, not one per spelling of its case. A
//! pattern with no gate after that goes to the fallback bucket. A fallback
//! pattern may still carry a weak gate (`FALLBACK_GATES`): exact literals of
//! at least 1 byte, derived the same way, that every match contains; a
//! fallback pattern without one is always a candidate.
//!
//! A fallback pattern is not left fully ungated when its structure still
//! requires some shorter text in every match (`;` of a connection string,
//! `Q~` of an Azure client secret): the same extraction at a one-byte
//! minimum gives it a *weak gate*, exact bytes the runtime looks for before
//! it runs that pattern's regex. A pattern with no such text stays an
//! always-candidate.

use regex_syntax::hir::literal::{ExtractKind, Extractor};
use regex_syntax::hir::{
    Capture, Class, ClassBytes, ClassBytesRange, ClassUnicode, ClassUnicodeRange, Hir, HirKind,
    Literal, Repetition,
};
use std::error::Error;
use std::fmt::Write as _;
use std::path::PathBuf;

// An inline module rooted at `src/security`, so `patterns.rs` finds its
// child slices in `src/security/patterns/` as it does in the library.
#[path = "src/security"]
mod security {
    pub(crate) mod patterns;
}
use security::patterns;

/// What a literal must be to gate a pattern: its shortest allowed length,
/// whether it is lowercased for an ASCII-case-insensitive search (otherwise
/// its bytes are searched exactly), and whether a run of parts of a
/// concatenation counts as a part ([`required_literals`]).
#[derive(Clone, Copy)]
struct Gate {
    min_len: usize,
    folded: bool,
    runs: bool,
}

/// The gate of the prescan automaton: literals of at least 3 bytes.
const STRONG: Gate = Gate {
    min_len: 3,
    folded: true,
    runs: false,
};

/// [`STRONG`] for a pattern it cannot gate, read with its ASCII capitals
/// folded ([`ascii_folded`]), runs of parts counted, and a set of more than
/// 64 literals cut to fit ([`fit`]): a `(?i)` label word the parser split
/// into one part per letter, in each branch of an alternation.
const FOLDED_RUNS: Gate = Gate {
    min_len: 3,
    folded: true,
    runs: true,
};

/// The gate of a fallback pattern: any literal, searched exactly.
const WEAK: Gate = Gate {
    min_len: 1,
    folded: false,
    runs: false,
};

/// The gate of `pattern`: its required prefix literals ([`prefix_literals`]),
/// else literals one of which occurs inside every match
/// ([`required_literals`]), else those of the pattern read as
/// [`FOLDED_RUNS`] reads it; `None` sends it to the fallback bucket. The
/// pattern is parsed with its flags: a `(?i)` pattern's literals carry the
/// Unicode case variants its regex matches.
fn gate_literals(pattern: &str) -> Result<Option<Vec<Vec<u8>>>, Box<dyn Error>> {
    let hir = regex_syntax::ParserBuilder::new()
        .utf8(false)
        .build()
        .parse(pattern)?;
    Ok(prefix_literals(&hir, STRONG)
        .or_else(|| required_literals(&hir, STRONG))
        .or_else(|| {
            let folded = ascii_folded(&hir);
            prefix_literals(&folded, FOLDED_RUNS)
                .or_else(|| required_literals(&folded, FOLDED_RUNS))
        }))
}

/// `hir` as the folded gate reads it: each ASCII capital of a literal or a
/// class is its small letter. The automaton ignores ASCII case, so a text
/// that holds a match of `hir` holds a match of this one in the automaton's
/// eyes, and a `(?i)` letter is one literal byte instead of two: only the
/// Unicode case variants (`ſ`, `K`) multiply the literals of a word, so a
/// label word stays within the extraction's limits.
fn ascii_folded(hir: &Hir) -> Hir {
    match hir.kind() {
        HirKind::Empty | HirKind::Look(_) => hir.clone(),
        HirKind::Literal(Literal(bytes)) => Hir::literal(bytes.to_ascii_lowercase()),
        HirKind::Class(Class::Unicode(class)) => {
            let capitals = ClassUnicode::new([ClassUnicodeRange::new('A', 'Z')]);
            let mut upper = class.clone();
            upper.intersect(&capitals);
            let mut folded = class.clone();
            folded.difference(&capitals);
            folded.union(&ClassUnicode::new(upper.iter().map(|range| {
                ClassUnicodeRange::new(
                    range.start().to_ascii_lowercase(),
                    range.end().to_ascii_lowercase(),
                )
            })));
            Hir::class(Class::Unicode(folded))
        }
        HirKind::Class(Class::Bytes(class)) => {
            let capitals = ClassBytes::new([ClassBytesRange::new(b'A', b'Z')]);
            let mut upper = class.clone();
            upper.intersect(&capitals);
            let mut folded = class.clone();
            folded.difference(&capitals);
            folded.union(&ClassBytes::new(upper.iter().map(|range| {
                ClassBytesRange::new(
                    range.start().to_ascii_lowercase(),
                    range.end().to_ascii_lowercase(),
                )
            })));
            Hir::class(Class::Bytes(folded))
        }
        HirKind::Repetition(repetition) => Hir::repetition(Repetition {
            sub: Box::new(ascii_folded(&repetition.sub)),
            ..repetition.clone()
        }),
        HirKind::Capture(capture) => Hir::capture(Capture {
            sub: Box::new(ascii_folded(&capture.sub)),
            ..capture.clone()
        }),
        HirKind::Concat(parts) => Hir::concat(parts.iter().map(ascii_folded).collect()),
        HirKind::Alternation(branches) => {
            Hir::alternation(branches.iter().map(ascii_folded).collect())
        }
    }
}

/// The weak gate of a fallback `pattern`: UTF-8 literals one of which occurs
/// inside every match, kept only when they are selective enough to be worth a
/// search per text: each at least 2 bytes, or a single literal.
fn weak_gate_literals(pattern: &str) -> Result<Option<Vec<String>>, Box<dyn Error>> {
    let hir = regex_syntax::ParserBuilder::new()
        .utf8(false)
        .build()
        .parse(pattern)?;
    let Some(literals) = prefix_literals(&hir, WEAK).or_else(|| required_literals(&hir, WEAK))
    else {
        return Ok(None);
    };
    let selective = literals.len() == 1 || literals.iter().all(|literal| literal.len() >= 2);
    let text: Option<Vec<String>> = literals
        .into_iter()
        .map(|literal| String::from_utf8(literal).ok())
        .collect();
    Ok(text.filter(|_| selective))
}

/// The prefix literals of `hir`, lowercased when `gate` folds: a finite set
/// one of which begins every match, or `None` when no trustworthy set exists
/// (infinite, more than 64 literals that [`fit`] cannot cut down, or an
/// anchor shorter than `gate.min_len`).
fn prefix_literals(hir: &Hir, gate: Gate) -> Option<Vec<Vec<u8>>> {
    let mut extractor = Extractor::new();
    extractor.kind(ExtractKind::Prefix);
    let seq = extractor.extract(hir);
    let literals = seq.literals()?; // None: infinite set, cannot gate
    let mut out: Vec<Vec<u8>> = Vec::new();
    for literal in literals {
        let bytes = literal.as_bytes();
        // A shorter anchor gates too weakly to be worth trusting.
        if bytes.len() < gate.min_len {
            return None;
        }
        let kept = if gate.folded {
            bytes.to_ascii_lowercase()
        } else {
            bytes.to_vec()
        };
        if !out.contains(&kept) {
            out.push(kept);
        }
    }
    (!out.is_empty())
        .then_some(out)
        .and_then(|out| fit(out, gate))
}

/// `literals` when they are at most 64; past that, for a gate that counts
/// runs ([`FOLDED_RUNS`]), each literal cut to the longest length, no
/// shorter than the gate's minimum, at which the distinct cuts are at most
/// 64. A text that holds a literal holds each of its prefixes, so the cut set
/// gates the same matches.
fn fit(literals: Vec<Vec<u8>>, gate: Gate) -> Option<Vec<Vec<u8>>> {
    if literals.len() <= 64 {
        return Some(literals);
    }
    if !gate.runs {
        return None;
    }
    let longest = literals.iter().map(Vec::len).max().unwrap_or(0);
    (gate.min_len..longest).rev().find_map(|len| {
        let mut cut: Vec<Vec<u8>> = Vec::new();
        for literal in &literals {
            let piece = literal[..len.min(literal.len())].to_vec();
            if !cut.contains(&piece) {
                cut.push(piece);
            }
        }
        (cut.len() <= 64).then_some(cut)
    })
}

/// A literal set one of which occurs in every match of `hir`, from its
/// structure: a match of a concatenation contains a match of each part (and,
/// where `gate` counts runs, of the run of parts from any one of them on: a
/// word the parser split into one part per letter is such a run), a match of
/// an alternation is a match of one branch, and a repetition with a minimum
/// of one contains its body. Each node's own prefix literals
/// ([`prefix_literals`]) qualify too; the set with the longest shortest
/// literal wins (then the smaller set).
fn required_literals(hir: &Hir, gate: Gate) -> Option<Vec<Vec<u8>>> {
    let structural = match hir.kind() {
        HirKind::Capture(capture) => required_literals(&capture.sub, gate),
        HirKind::Repetition(repetition) if repetition.min >= 1 => {
            required_literals(&repetition.sub, gate)
        }
        HirKind::Concat(parts) => (0..parts.len())
            .flat_map(|start| {
                let run = (gate.runs && start > 0)
                    .then(|| prefix_literals(&Hir::concat(parts[start..].to_vec()), gate))
                    .flatten();
                [required_literals(&parts[start], gate), run]
            })
            .flatten()
            .max_by_key(|set| strength(set)),
        HirKind::Alternation(branches) => {
            let mut union: Vec<Vec<u8>> = Vec::new();
            for branch in branches {
                for literal in required_literals(branch, gate)? {
                    if !union.contains(&literal) {
                        union.push(literal);
                    }
                }
            }
            fit(union, gate)
        }
        _ => None,
    };
    [prefix_literals(hir, gate), structural]
        .into_iter()
        .flatten()
        .max_by_key(|set| strength(set))
}

/// How selective a gate is: its shortest literal, then fewer literals.
fn strength(set: &[Vec<u8>]) -> (usize, std::cmp::Reverse<usize>) {
    (
        set.iter().map(Vec::len).min().unwrap_or(0),
        std::cmp::Reverse(set.len()),
    )
}

/// `bytes` as a Rust byte-string literal.
fn byte_string(bytes: &[u8]) -> String {
    let mut out = String::from("b\"");
    for &byte in bytes {
        match byte {
            b'"' | b'\\' => {
                out.push('\\');
                out.push(byte as char);
            }
            0x20..=0x7e => out.push(byte as char),
            _ => {
                let _ = write!(out, "\\x{byte:02x}");
            }
        }
    }
    out.push('"');
    out
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src/security/patterns.rs");
    println!("cargo::rerun-if-changed=src/security/patterns");
    let mut gates = String::new();
    let mut fallback_gates = String::new();
    let mut longest = 0usize;
    for (index, pattern) in patterns::PATTERNS.iter().enumerate() {
        let hir = regex_syntax::ParserBuilder::new()
            .utf8(false)
            .build()
            .parse(pattern.regex)
            .map_err(|error| format!("secret pattern {}: {error}", pattern.name))?;
        longest = longest.max(hir.properties().maximum_len().unwrap_or(0));
        match gate_literals(pattern.regex)
            .map_err(|error| format!("secret pattern {}: {error}", pattern.name))?
        {
            Some(literals) => {
                for literal in literals {
                    let _ = writeln!(gates, "    ({}, {index}),", byte_string(&literal));
                }
            }
            None => {
                let weak = weak_gate_literals(pattern.regex)
                    .map_err(|error| format!("secret pattern {}: {error}", pattern.name))?
                    .unwrap_or_default();
                let literals: Vec<String> =
                    weak.iter().map(|literal| format!("{literal:?}")).collect();
                let _ = writeln!(fallback_gates, "    ({index}, &[{}]),", literals.join(", "));
            }
        }
    }
    let output = format!(
        "// @generated by crates/engine/build.rs from src/security/patterns.rs.\n\
         /// Every gated pattern's lowercased prefix literal and the index of the\n\
         /// pattern it belongs to, in pattern order.\n\
         pub(super) static GATE_LITERALS: &[(&[u8], usize)] = &[\n{gates}];\n\
         /// Each fallback pattern with its weak gate: text one of whose entries\n\
         /// occurs in every match, or no entry for a pattern with none.\n\
         pub(super) static FALLBACK_GATES: &[(usize, &[&str])] = &[\n{fallback_gates}];\n\
         /// Bytes of the longest match any pattern with a bound can make.\n\
         pub const LONGEST_BOUNDED_MATCH: usize = {longest};\n",
    );
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    std::fs::write(out_dir.join("secret_gates.rs"), output)?;
    Ok(())
}
