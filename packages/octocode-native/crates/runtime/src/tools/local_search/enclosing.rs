//! Enclosing declarations of search hits, from the same declaration outline
//! behind astSearch `symbols` (and its content-keyed cache).
//!
//! A hit row names the innermost declaration around its line as
//! `enclosing: {symbolName, kind, line, endLine}` (`line` is the name line),
//! so a caller can cite or address the function a hit sits in without
//! reading it: `symbolName` + `line` anchor lspSearch, `line`–`endLine` is a
//! localFetch range. A hit on a declaration's own name line names the
//! declaration around that one. Every hit row inside a declaration names
//! it, so no row reads as having none. A declaration search (`fn foo`)
//! names the owners of declaring hits only. A searched symbol whose
//! declaration is among the hits also yields a ready lspSearch references
//! lead.

use super::{handoff::*, types::*};
use crate::security::ContentSecurity;
use sha2::{Digest, Sha256};

/// One declaration of a file's outline, 1-based lines.
struct Declaration {
    kind: String,
    name: String,
    /// The name line.
    line: u32,
    start: u32,
    end: u32,
}

pub(super) struct Outline {
    declarations: Vec<Declaration>,
}

impl Outline {
    /// The outline of `source`; `None` when no extractor supports the file.
    /// It reads the typed declarations parse every reader of the file
    /// shares ([`crate::tools::declarations_cache::file_document`]): one
    /// parse per source version, never a JSON round trip.
    pub(super) fn of(source: &str, canonical_path: &str) -> Option<Self> {
        let document = crate::tools::declarations_cache::file_document(source, canonical_path)?;
        let declarations = document
            .declarations
            .iter()
            .filter(|item| !item.name.is_empty())
            .map(|item| {
                let start = item.range.start.line.saturating_add(1);
                Declaration {
                    kind: item.kind.clone(),
                    name: item.name.clone(),
                    line: item.selection_range.start.line.saturating_add(1),
                    start,
                    end: item.range.end.line.saturating_add(1).max(start),
                }
            })
            .collect();
        Some(Self { declarations })
    }

    /// The innermost declaration spanning `line` other than one named on
    /// `line` itself (its index in the outline).
    pub(super) fn owner(&self, line: u32) -> Option<usize> {
        self.declarations
            .iter()
            .enumerate()
            .filter(|(_, d)| d.start <= line && line <= d.end && d.line != line)
            .min_by_key(|(_, d)| (d.end - d.start, std::cmp::Reverse(d.start)))
            .map(|(index, _)| index)
    }

    /// Declaration `owner`: its name, kind, name line and last line, so a
    /// read of it needs no outline first.
    pub(super) fn label(&self, owner: usize) -> Option<super::types::Enclosing> {
        let declaration = self.declarations.get(owner)?;
        Some(super::types::Enclosing {
            symbol_name: declaration.name.clone(),
            kind: declaration.kind.clone(),
            line: declaration.line,
            end_line: declaration.end.max(declaration.line),
        })
    }

    /// The innermost declaration spanning `line` other than one named on
    /// `line` itself.
    #[cfg(test)]
    pub(super) fn enclosing(&self, line: u32) -> Option<super::types::Enclosing> {
        self.label(self.owner(line)?)
    }

    /// Whether `line` is the name line of a declaration named `name`.
    pub(super) fn declares(&self, name: &str, line: u32) -> bool {
        self.declaration(name, line).is_some()
    }

    /// The declaration named `name` on name line `line`: its kind and its
    /// first and last line.
    pub(super) fn declaration(&self, name: &str, line: u32) -> Option<(&str, u32, u32)> {
        self.declarations
            .iter()
            .find(|d| d.line == line && d.name == name)
            .map(|d| (d.kind.as_str(), d.start, d.end))
    }
}

/// Whether the search names a declaration (`fn foo`, `class Foo`) rather
/// than a bare identifier.
pub(super) fn declaration_search(search_text: &str) -> bool {
    searched_symbol(search_text).is_some() && search_text.trim().contains(char::is_whitespace)
}

/// The identifier a search names: a bare identifier, or one after a
/// declaration keyword.
pub(super) fn searched_symbol(search_text: &str) -> Option<&str> {
    let text = search_text.trim();
    let name = match text.split_once(char::is_whitespace) {
        Some((keyword, rest)) if octocode_engine::search::is_declaration_keyword(keyword) => {
            rest.trim()
        }
        Some(_) => return None,
        None => text,
    };
    let mut chars = name.chars();
    let first = chars.next()?;
    ((first.is_alphabetic() || first == '_' || first == '$')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$'))
    .then_some(name)
}

/// Serialized chars an `enclosing` field adds besides its value.
pub(super) const ENCLOSING_FIELD_CHARS: usize = ",\"enclosing\":".len();

/// What a page cut reserves per hit row of a file it has not outlined (a
/// later page's, or one it could not read; see
/// [`super::layout::NamePricing`]) for its `enclosing` name: the field, the
/// object's keys, a kind, two line numbers and a typical symbol name. A
/// longer name still goes on its row; the page runs that much over budget.
pub(super) const ENCLOSING_RESERVE_CHARS: usize = ENCLOSING_FIELD_CHARS + 80;

/// The current outline of a shown file, or `None` when it is too large,
/// unreadable, unsupported, or no longer the bytes a stored scan matched.
pub(super) fn shown_outline(
    source: &std::path::Path,
    expected: super::sources::Expected,
    security: &ContentSecurity,
) -> Option<super::enclosing::Outline> {
    let bytes = shown_bytes(source, expected)?;
    outline_of(&bytes, source, security)
}

/// The current bytes of a shown file within the parse cap, or `None` when
/// it is larger, unreadable, no longer a regular file (a FIFO or link
/// swapped in since the walk), or no longer the bytes a stored scan matched.
pub(super) fn shown_bytes(
    source: &std::path::Path,
    expected: super::sources::Expected,
) -> Option<Vec<u8>> {
    let limit = octocode_engine::signatures::MAX_PARSE_SIZE;
    let bytes = super::sources::read_entry(source, limit)?;
    if let super::sources::Expected::Stored(stored) = expected
        && stored != Some(<[u8; 32]>::from(Sha256::digest(&bytes)))
    {
        return None;
    }
    Some(bytes)
}

/// Set `matchKeys` on each shown JSON or TOML file with hit rows: the key
/// path of every hit line, read from the file's current bytes, in page
/// order while the page stays within `page_budget`. Returns the note for
/// files whose keys did not fit, never dropped silently.
pub(super) fn annotate_match_keys(
    files: &mut [SearchFile],
    output_root: &std::path::Path,
    page_budget: usize,
    expected: &dyn Fn(&std::path::Path) -> super::sources::Expected,
    security: &ContentSecurity,
) -> Option<String> {
    let mut used = octocode_engine::json_size::bytes(&*files).unwrap_or(usize::MAX);
    let mut over_budget = 0usize;
    for file in files.iter_mut() {
        let source = output_root.join(&file.path);
        let Some(rows) = file.matches.as_ref().filter(|rows| !rows.is_empty()) else {
            continue;
        };
        if !crate::tools::match_keys::is_keyed_path(&source) {
            continue;
        }
        let mut lines = rows
            .iter()
            .flat_map(row_lines)
            .filter(|line| *line > 0)
            .map(|line| line as usize)
            .collect::<Vec<_>>();
        lines.sort_unstable();
        lines.dedup();
        let limit = octocode_engine::signatures::MAX_PARSE_SIZE;
        let Some(bytes) = shown_bytes(&source, expected(&source)) else {
            continue;
        };
        let Ok(text) = security.decode_source_bytes(&bytes, limit) else {
            continue;
        };
        let keys = crate::tools::match_keys::line_key_paths(&source, &text, &lines);
        if keys.is_empty() {
            continue;
        }
        let added = octocode_engine::json_size::bytes(&crate::tools::match_keys::MatchKeys(&keys))
            .unwrap_or(usize::MAX)
            .saturating_add(MATCH_KEYS_FIELD_CHARS);
        if over_budget == 0 && used.saturating_add(added) <= page_budget {
            used += added;
            file.match_keys = keys;
        } else {
            over_budget += 1;
        }
    }
    (over_budget > 0).then(|| {
        let noun = if over_budget == 1 { "file" } else { "files" };
        format!(
            "Key paths (matchKeys) omitted on {over_budget} later {noun} to keep the page within its size budget."
        )
    })
}

/// Serialized chars around a file's key map.
const MATCH_KEYS_FIELD_CHARS: usize = ",\"matchKeys\":".len();

/// Largest file outlined for `enclosing` names. Parsing is linear (a 4.5 MB
/// generated Rust file took 1 s of a 1.2 s search, 2026-10-08) and files this
/// large are almost always generated; their hits keep no enclosing name.
pub(super) const ENCLOSING_MAX_BYTES: usize = 1024 * 1024;

/// The outline of a shown file's `bytes`, read within the parse cap; none
/// for a file over [`ENCLOSING_MAX_BYTES`].
pub(super) fn outline_of(
    bytes: &[u8],
    source: &std::path::Path,
    security: &ContentSecurity,
) -> Option<super::enclosing::Outline> {
    if bytes.len() > ENCLOSING_MAX_BYTES {
        return None;
    }
    let limit = octocode_engine::signatures::MAX_PARSE_SIZE;
    let text = security.decode_source_bytes(bytes, limit).ok()?;
    super::enclosing::Outline::of(&text, &source.to_string_lossy())
}

/// Lines a shown row stands for: every matched line of a merged block.
pub(super) fn row_lines(row: &SearchMatch) -> Vec<u32> {
    row.match_lines.clone().unwrap_or_else(|| vec![row.line])
}

/// Name the enclosing declaration of every hit row of a page's files
/// and find the first shown hit that declares `symbol`. Each hit carries
/// the complete declaration, even when a response splits the file.
/// Each file is parsed once. The page cut charged the names' size, so a
/// name is never traded for budget; rows of a file over 1 MiB are counted
/// in the returned note, never dropped silently.
pub(super) fn annotate_enclosing(
    files: &mut [SearchFile],
    symbol: Option<&str>,
    declaring: Option<&str>,
    output_root: &std::path::Path,
    expected: &dyn Fn(&std::path::Path) -> super::sources::Expected,
    mut outlines: super::verify::Outlines,
    security: &ContentSecurity,
) -> (Option<Definition>, Option<String>) {
    let mut names: Vec<Vec<Option<super::types::Enclosing>>> = Vec::with_capacity(files.len());
    let mut definition = None;
    let mut oversized_rows = 0usize;
    for file in files.iter() {
        let Some(matches) = file.matches.as_ref().filter(|rows| !rows.is_empty()) else {
            names.push(Vec::new());
            continue;
        };
        let source = output_root.join(&file.path);
        // The secret check already read and outlined most shown files.
        let outline = match outlines.remove(&source) {
            Some(outline) => outline,
            None => shown_outline(&source, expected(&source), security),
        };
        let Some(outline) = outline else {
            // Only a file whose language has declarations lost them to its
            // size (a 3 MiB `uv.lock` never had any).
            if outlined_language(&source)
                && std::fs::metadata(&source)
                    .is_ok_and(|meta| meta.len() > ENCLOSING_MAX_BYTES as u64)
            {
                oversized_rows += matches.len();
            }
            names.push(Vec::new());
            continue;
        };
        if definition.is_none()
            && let Some(symbol) = symbol
            && let Some((line, (kind, start, end))) = matches
                .iter()
                .flat_map(row_lines)
                .find_map(|line| Some((line, outline.declaration(symbol, line)?)))
        {
            definition = Some(Definition {
                source: source.to_string_lossy().into_owned(),
                line,
                kind: kind.to_owned(),
                start,
                end,
            });
        }
        names.push(row_owners(&outline, matches, declaring));
    }
    for (file, names) in files.iter_mut().zip(names) {
        for (row, name) in file.matches.iter_mut().flatten().zip(names) {
            row.enclosing = name;
        }
    }

    let note = (oversized_rows > 0).then(|| {
        format!(
            "Enclosing declarations (enclosing) are not named in files over 1 MiB; {} there {} none.",
            hit_rows(oversized_rows),
            have(oversized_rows)
        )
    });
    (definition, note)
}

/// `1 hit row`, `3 hit rows`.
fn hit_rows(rows: usize) -> String {
    format!("{rows} hit {}", if rows == 1 { "row" } else { "rows" })
}

pub(super) fn have(rows: usize) -> &'static str {
    if rows == 1 { "has" } else { "have" }
}

/// Whether `source`'s language has a declaration outline (by extension).
pub(super) fn outlined_language(source: &std::path::Path) -> bool {
    source
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            octocode_engine::signatures::languages::find_entry(&extension.to_ascii_lowercase())
                .is_some()
        })
}

/// Each row's enclosing declaration, on every row inside one: so no row
/// reads as having none. With `declaring` (a declaration search), only rows that declare
/// that symbol are named.
pub(super) fn row_owners(
    outline: &super::enclosing::Outline,
    rows: &[SearchMatch],
    declaring: Option<&str>,
) -> Vec<Option<super::types::Enclosing>> {
    rows.iter()
        .map(|row| {
            let named = declaring.is_none_or(|symbol| {
                row_lines(row)
                    .iter()
                    .any(|line| outline.declares(symbol, *line))
            });
            named
                .then(|| outline.owner(row_lines(row)[0]))
                .flatten()
                .and_then(|owner| outline.label(owner))
        })
        .collect()
}

#[cfg(test)]
mod tests;
