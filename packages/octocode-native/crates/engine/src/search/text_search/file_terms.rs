//! `file_contains_all` / `file_contains_none`: file-level terms checked against the searched bytes.

use crate::types::TextSearchOptions;
use aho_corasick::AhoCorasick;

/// Bytes read to check one file's file-level terms.
pub(super) const FILE_TERMS_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// The bytes a search read from a file, and whether it read to the end.
pub(super) struct CopiedRead {
    pub(super) bytes: Vec<u8>,
    pub(super) eof: bool,
}

/// `file_contains_all` / `file_contains_none`, compiled once per search.
/// Each term is smart-case on its own: one with an uppercase letter is
/// case-sensitive, an all-lowercase one matches any case. Terms split into one
/// automaton per case mode; pattern `i` of each is `terms[i]`.
pub(super) struct FileTerms {
    automatons: Vec<(AhoCorasick, Vec<FileTerm>)>,
    required: usize,
}

#[derive(Clone, Copy)]
enum FileTerm {
    Required(usize),
    Excluded,
}

impl FileTerms {
    pub(super) fn new(opts: &TextSearchOptions) -> Option<Self> {
        let all = opts.file_contains_all.as_deref().unwrap_or_default();
        let none = opts.file_contains_none.as_deref().unwrap_or_default();
        let required: Vec<&str> = all
            .iter()
            .map(String::as_str)
            .filter(|t| !t.is_empty())
            .collect();
        let terms: Vec<(&str, FileTerm)> = required
            .iter()
            .enumerate()
            .map(|(index, term)| (*term, FileTerm::Required(index)))
            .chain(
                none.iter()
                    .map(String::as_str)
                    .filter(|term| !term.is_empty())
                    .map(|term| (term, FileTerm::Excluded)),
            )
            .collect();
        if terms.is_empty() {
            return None;
        }
        let automatons = [false, true]
            .into_iter()
            .filter_map(|insensitive| {
                let (patterns, kinds): (Vec<&str>, Vec<FileTerm>) = terms
                    .iter()
                    .filter(|(term, _)| term.bytes().any(|b| b.is_ascii_uppercase()) != insensitive)
                    .copied()
                    .unzip();
                let matcher = AhoCorasick::builder()
                    .ascii_case_insensitive(insensitive)
                    .build(&patterns)
                    .ok()?;
                (!patterns.is_empty()).then_some((matcher, kinds))
            })
            .collect();
        Some(Self {
            automatons,
            required: required.len(),
        })
    }

    /// Whether the searched read's bytes hold every required term and no
    /// excluded one: the copy the search kept, completed from the open file
    /// when the search stopped before its end. A failed read fails the check.
    pub(super) fn pass(&self, copied: Option<CopiedRead>, file: &std::fs::File) -> bool {
        use std::io::{Read, Seek, SeekFrom};
        let Some(CopiedRead { mut bytes, eof }) = copied else {
            return false;
        };
        let len = bytes.len() as u64;
        if !eof && len < FILE_TERMS_MAX_BYTES {
            let mut handle = file;
            let rest = handle.seek(SeekFrom::Start(len)).and_then(|_| {
                handle
                    .take(FILE_TERMS_MAX_BYTES - len)
                    .read_to_end(&mut bytes)
            });
            if rest.is_err() {
                return false;
            }
        }
        let mut found = vec![false; self.required];
        for (matcher, kinds) in &self.automatons {
            for hit in matcher.find_overlapping_iter(&bytes) {
                match kinds[hit.pattern().as_usize()] {
                    FileTerm::Excluded => return false,
                    FileTerm::Required(index) => found[index] = true,
                }
            }
        }
        found.into_iter().all(|hit| hit)
    }
}
