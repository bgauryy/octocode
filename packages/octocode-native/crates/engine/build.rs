//! Build script: derives the secret detector's prescan gate from the pattern
//! table, so no process parses about 300 pattern regexes at startup.
//!
//! For each pattern in `src/security/patterns.rs` (joined from its slices in
//! `src/security/patterns/`) it extracts a gate with regex-syntax: a finite
//! set of literals (each at least 3 bytes, at most 64 of them) one of which
//! occurs in every match. The *required prefix
//! literals* come first; a pattern without them gates on literals its
//! structure requires inside the match. A pattern with neither goes to the
//! fallback bucket, which is always a candidate. Patterns are parsed with
//! their flags, so a `(?i)` pattern's literals include every Unicode case
//! variant the regex matches (`ſ` for `s`, `K` (U+212A) for `k`).
//! The runtime builds one ASCII-case-insensitive aho-corasick automaton over
//! the lowercased literals written to `OUT_DIR/secret_gates.rs`.

use regex_syntax::hir::literal::{ExtractKind, Extractor};
use regex_syntax::hir::{Hir, HirKind};
use std::error::Error;
use std::fmt::Write as _;
use std::path::PathBuf;

// An inline module rooted at `src/security`, so `patterns.rs` finds its
// child slices in `src/security/patterns/` as it does in the library.
#[path = "src/security"]
mod security {
    #[expect(
        dead_code,
        reason = "the gate reads only each pattern's name and regex"
    )]
    pub(crate) mod patterns;
}
use security::patterns;

/// The gate of `pattern`: its required prefix literals ([`prefix_literals`]),
/// else literals one of which occurs inside every match
/// ([`required_literals`]); `None` sends it to the fallback bucket. The
/// pattern is parsed with its flags: a `(?i)` pattern's literals carry the
/// Unicode case variants its regex matches.
fn gate_literals(pattern: &str) -> Result<Option<Vec<Vec<u8>>>, Box<dyn Error>> {
    let hir = regex_syntax::ParserBuilder::new()
        .utf8(false)
        .build()
        .parse(pattern)?;
    Ok(prefix_literals(&hir).or_else(|| required_literals(&hir)))
}

/// The prefix literals of `hir`, lowercased: a finite set one of which
/// begins every match, or `None` when no trustworthy set exists (infinite,
/// more than 64 literals, or an anchor shorter than 3 bytes).
fn prefix_literals(hir: &Hir) -> Option<Vec<Vec<u8>>> {
    let mut extractor = Extractor::new();
    extractor.kind(ExtractKind::Prefix);
    let seq = extractor.extract(hir);
    let literals = seq.literals()?; // None: infinite set, cannot gate
    let mut out: Vec<Vec<u8>> = Vec::new();
    for literal in literals {
        let bytes = literal.as_bytes();
        // A shorter anchor gates too weakly to be worth trusting.
        if bytes.len() < 3 {
            return None;
        }
        let lowered = bytes.to_ascii_lowercase();
        if !out.contains(&lowered) {
            out.push(lowered);
        }
    }
    (!out.is_empty() && out.len() <= 64).then_some(out)
}

/// A literal set one of which occurs in every match of `hir`, from its
/// structure: a match of a concatenation contains a match of each part, a
/// match of an alternation is a match of one branch, and a repetition with
/// a minimum of one contains its body. Each node's own prefix literals
/// ([`prefix_literals`]) qualify too; the set with the longest shortest
/// literal wins (then the smaller set).
fn required_literals(hir: &Hir) -> Option<Vec<Vec<u8>>> {
    let structural = match hir.kind() {
        HirKind::Capture(capture) => required_literals(&capture.sub),
        HirKind::Repetition(repetition) if repetition.min >= 1 => {
            required_literals(&repetition.sub)
        }
        HirKind::Concat(parts) => parts
            .iter()
            .filter_map(required_literals)
            .max_by_key(|set| strength(set)),
        HirKind::Alternation(branches) => {
            let mut union: Vec<Vec<u8>> = Vec::new();
            for branch in branches {
                for literal in required_literals(branch)? {
                    if !union.contains(&literal) {
                        union.push(literal);
                    }
                }
            }
            (union.len() <= 64).then_some(union)
        }
        _ => None,
    };
    [prefix_literals(hir), structural]
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
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/security/patterns.rs");
    println!("cargo:rerun-if-changed=src/security/patterns");
    let mut gates = String::new();
    let mut fallback = Vec::new();
    for (index, pattern) in patterns::PATTERNS.iter().enumerate() {
        match gate_literals(pattern.regex)
            .map_err(|error| format!("secret pattern {}: {error}", pattern.name))?
        {
            Some(literals) => {
                for literal in literals {
                    let _ = writeln!(gates, "    ({}, {index}),", byte_string(&literal));
                }
            }
            None => fallback.push(index.to_string()),
        }
    }
    let output = format!(
        "// @generated by crates/engine/build.rs from src/security/patterns.rs.\n\
         /// Every gated pattern's lowercased prefix literal and the index of the\n\
         /// pattern it belongs to, in pattern order.\n\
         pub(super) static GATE_LITERALS: &[(&[u8], usize)] = &[\n{gates}];\n\
         /// Patterns with no provable literal: always candidates.\n\
         pub(super) static FALLBACK: &[usize] = &[{}];\n",
        fallback.join(", ")
    );
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    std::fs::write(out_dir.join("secret_gates.rs"), output)?;
    Ok(())
}
