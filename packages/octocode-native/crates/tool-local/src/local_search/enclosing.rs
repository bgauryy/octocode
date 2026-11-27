//! Enclosing declarations of search hits, from the same declaration outline
//! behind astSearch `symbols`.
//!
//! A hit row names the innermost declaration around its line as
//! `declarations: [{symbolName, kind, line, endLine}]` on the file and an
//! `enclosing` index on the hit (`line` is the name line; its attribute and
//! doc lines before it are an astSearch `symbols` read away); the
//! declarations holding that one (a method's class, an impl's module) are
//! listed before it, outermost first, so every declaration holding a hit
//! is named,
//! so a caller can cite or address the function a hit sits in without
//! reading it: `symbolName` + `line` anchor lspSearch, `line`–`endLine` is a
//! localFetch range. Containment is by (line, byte), so on a one-line
//! (minified) file each hit names the declaration its byte sits in. A hit
//! on a declaration's name line up to the end of its name names the
//! declaration around that one. Every hit row inside a declaration names
//! it, so no row reads as having none. A declaration search (`fn foo`)
//! names the owners of declaring hits only.
//!
//! A hit whose matched bytes cover a declaration's name, or part of it,
//! declares it, wherever the match starts (`struct Histogram {` and
//! `Histogram` alike; a match that ends before the name, `export` alone,
//! declares nothing): the file lists that declaration too and the hit names
//! it by a `declaration` index, so its `line`–`endLine` is the range that
//! reads it. The outline is the one source of that fact: a hit the outline
//! cannot place (a local binding, a language without an extractor, a file
//! over [`ENCLOSING_MAX_BYTES`]) carries no `declaration`.
//!
//! A Markdown file's outline is its sections
//! ([`octocode_tools_shared::source_view::sections`], the ranges a
//! `block:true` read returns): a hit names the innermost section holding
//! it alone, a hit on a heading line the section that heading opens, so
//! one entry per hit states the range that reads its section.

use super::types::*;
use octocode_runtime_base::security::ContentSecurity;

mod match_keys;
pub(super) use match_keys::annotate_match_keys;

/// A source position: 1-based line, then the UTF-8 byte within it (the
/// unit of a hit's `byte_offset`).
type At = (u32, u32);

/// One declaration of a file's outline, 1-based lines.
#[derive(Clone)]
struct Declaration {
    kind: String,
    name: String,
    /// The name line.
    line: u32,
    /// The last line.
    end: u32,
    /// Where the declaration starts and ends (exclusive), and where its
    /// name starts and ends: what [`Outline::owner`] and
    /// [`Outline::declared`] compare a hit with.
    from: At,
    to: At,
    name_from: At,
    name_end: At,
}

#[derive(Clone)]
pub(super) struct Outline {
    declarations: Vec<Declaration>,
    /// The declarations are a Markdown file's sections ([`Self::sections`]):
    /// a hit names one of them, never the sections around it.
    sections: bool,
}

/// The `kind` a Markdown section is listed by.
const SECTION_KIND: &str = "section";

/// Outlines by the digest of the text they outline and its path, so a text
/// a page, a later page or a repeat already outlined is not parsed again.
static OUTLINES: octocode_tools_shared::page_memo::PageMemo<std::sync::Arc<Outline>> =
    octocode_tools_shared::page_memo::PageMemo::new();

impl Outline {
    /// The heap this outline holds.
    fn weight(&self) -> usize {
        std::mem::size_of::<Self>()
            + self
                .declarations
                .iter()
                .map(|item| {
                    std::mem::size_of::<Declaration>() + item.kind.capacity() + item.name.capacity()
                })
                .sum::<usize>()
    }

    /// The outline of `source`; `None` when no extractor supports the file
    /// and it is no Markdown file ([`Self::sections`]).
    /// It reads the typed declarations document of one fresh parse
    /// ([`octocode_tools_shared::declarations::file_document`]), owned by this
    /// request and never a JSON round trip.
    pub(super) fn of(source: &str, canonical_path: &str) -> Option<Self> {
        let Some(document) =
            octocode_tools_shared::declarations::file_document(source, canonical_path)
        else {
            return Self::sections(source, canonical_path);
        };
        // Graph positions count UTF-16 columns on `\n` lines; hits count
        // bytes on the same lines.
        let index = octocode_engine::text::LineIndex::tree_sitter(source);
        let at = |position: &octocode_engine::graph::GraphPosition| -> At {
            let byte = index.position_to_byte(source, position.line, position.character);
            let line_start = index
                .line_start(position.line as usize)
                .map_or(0, octocode_engine::num::saturating_u32);
            (
                position.line.saturating_add(1),
                byte.saturating_sub(line_start),
            )
        };
        let declarations = document
            .declarations
            .iter()
            .filter(|item| !item.name.is_empty())
            .map(|item| {
                let start = item.range.start.line.saturating_add(1);
                let from = at(&item.range.start);
                Declaration {
                    kind: item.kind.clone(),
                    name: item.name.clone(),
                    line: item.selection_range.start.line.saturating_add(1),
                    end: item.range.end.line.saturating_add(1).max(start),
                    from,
                    to: at(&item.range.end).max(from),
                    name_from: at(&item.selection_range.start),
                    name_end: at(&item.selection_range.end),
                }
            })
            .collect();
        Some(Self {
            declarations,
            sections: false,
        })
    }

    /// The outline of a Markdown file: its sections, the ones a block read
    /// widens to ([`octocode_tools_shared::source_view::sections`]). A
    /// section's name is its whole heading line, so any hit on that line
    /// declares it.
    fn sections(source: &str, canonical_path: &str) -> Option<Self> {
        use octocode_engine::num::saturating_u32;
        let declarations =
            octocode_tools_shared::source_view::sections::sections(source, canonical_path)?
                .into_iter()
                .filter(|section| !section.name.is_empty())
                .map(|section| {
                    let (line, end) = (saturating_u32(section.line), saturating_u32(section.end));
                    Declaration {
                        kind: SECTION_KIND.to_owned(),
                        name: section.name,
                        line,
                        end,
                        from: (line, 0),
                        to: (end.saturating_add(1), 0),
                        name_from: (line, 0),
                        name_end: (line.saturating_add(1), 0),
                    }
                })
                .collect();
        Some(Self {
            declarations,
            sections: true,
        })
    }

    /// The innermost declaration whose range holds the hit at `byte` of
    /// `line`, other than one whose header the hit sits in up to its name
    /// (its index in the outline). A hit at a declaration's end is past it;
    /// an empty range holds the hits of its own line. A hit on a Markdown
    /// heading has none: the section it declares is its one entry.
    pub(super) fn owner(&self, line: u32, byte: u32) -> Option<usize> {
        let hit = (line, byte);
        if self.sections && self.declarations.iter().any(|d| d.line == line) {
            return None;
        }
        self.declarations
            .iter()
            .enumerate()
            .filter(|(_, d)| {
                let holds = d.from <= hit && (hit < d.to || (d.from == d.to && line == d.to.0));
                holds && !(line == d.line && hit < d.name_end)
            })
            .min_by_key(|(_, d)| (std::cmp::Reverse(d.from), d.to))
            .map(|(index, _)| index)
    }

    /// Declaration `owner` after the declarations holding it, outermost
    /// first: a method after its class. A declaration whose range equals
    /// the owner's is not a holder of it. A Markdown section is listed
    /// alone: its range reads it, and the sections around it are a read of
    /// the file's outline away.
    pub(super) fn chain(&self, owner: usize) -> Vec<super::types::Enclosing> {
        let Some(inner) = self.declarations.get(owner) else {
            return Vec::new();
        };
        if self.sections {
            return self.label(owner).into_iter().collect();
        }
        let mut holders = self
            .declarations
            .iter()
            .enumerate()
            .filter(|(index, d)| {
                *index != owner
                    && d.from <= inner.from
                    && inner.to <= d.to
                    && (d.from, d.to) != (inner.from, inner.to)
            })
            .collect::<Vec<_>>();
        holders.sort_by_key(|(_, d)| (d.from, std::cmp::Reverse(d.to)));
        holders
            .into_iter()
            .map(|(index, _)| index)
            .chain([owner])
            .filter_map(|index| self.label(index))
            .collect()
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

    /// The innermost declaration holding the hit at `byte` of `line`
    /// ([`Self::owner`]).
    #[cfg(test)]
    pub(super) fn enclosing(&self, line: u32, byte: u32) -> Option<super::types::Enclosing> {
        self.label(self.owner(line, byte)?)
    }

    /// Whether `line` is the name line of a declaration named `name`.
    pub(super) fn declares(&self, name: &str, line: u32) -> bool {
        self.named(name, line).is_some()
    }

    /// The declaration named `name` on name line `line` (its index in the
    /// outline).
    fn named(&self, name: &str, line: u32) -> Option<usize> {
        self.declarations
            .iter()
            .position(|d| d.line == line && d.name == name)
    }

    /// The declaration the hit declares (its index in the outline): the
    /// one whose name the matched bytes, `len` from `byte` of `line`, cover
    /// or reach into, the first such name on the line. A match that ends
    /// before the name (a modifier or keyword alone) declares nothing. A
    /// hit of no known length is the byte it starts at.
    pub(super) fn declared(&self, line: u32, byte: u32, len: u32) -> Option<usize> {
        let (from, to) = ((line, byte), (line, byte.saturating_add(len.max(1))));
        self.declarations
            .iter()
            .enumerate()
            .filter(|(_, d)| d.name_from < to && from < d.name_end)
            .min_by_key(|(_, d)| d.name_from)
            .map(|(index, _)| index)
    }
}

/// The symbol a declaration search of `query` names (`fn foo`: `foo`),
/// of which only the rows declaring it name their owners
/// ([`row_owners`]); `None` for any other search, and for one whose pattern
/// does not read as written (case-insensitive, PCRE2 or inverted).
pub(super) fn declaring_symbol(query: &octocode_contract_types::LocalSearchQuery) -> Option<&str> {
    use super::local_search_query;
    let pattern = local_search_query::pattern(query);
    (local_search_query::case_mode(query) != octocode_contract_types::CaseMode::Insensitive
        && local_search_query::regex_mode(query) != octocode_contract_types::RegexEngine::Pcre2
        && local_search_query::invert_match(query) != Some(true)
        && declaration_search(pattern))
    .then(|| searched_symbol(pattern))
    .flatten()
}

/// Whether the search names a declaration (`fn foo`, `pub fn foo`,
/// `class Foo`) rather than a bare identifier.
pub(super) fn declaration_search(search_text: &str) -> bool {
    searched_symbol(search_text).is_some() && search_text.trim().contains(char::is_whitespace)
}

/// The identifier a search names: a bare identifier, or the name a
/// declaration head declares ([`octocode_engine::search::declared_name`]:
/// `fn foo`, `pub(crate) fn foo`, `export default class Foo`), read by the
/// walk that marks a hit line `declaration`; a head written up to its
/// signature's opening (`pub fn foo(`) names `foo`.
pub(super) fn searched_symbol(search_text: &str) -> Option<&str> {
    let text = search_text.trim();
    let name = if text.contains(char::is_whitespace) {
        octocode_engine::search::declared_name(text)?
    } else {
        text
    };
    let mut chars = name.chars();
    let first = chars.next()?;
    ((first.is_alphabetic() || first == '_' || first == '$')
        && chars.all(octocode_engine::text::is_ident_char))
    .then_some(name)
}

/// Serialized chars an `enclosing` field adds besides its value.
pub(super) const ENCLOSING_FIELD_CHARS: usize = ",\"enclosing\":".len();

/// Serialized chars a `declaration` field adds besides its value.
pub(super) const DECLARATION_FIELD_CHARS: usize = ",\"declaration\":".len();

/// Serialized chars one declaration takes in its file's `declarations`,
/// with its comma: the line-first outline string the response stage lists
/// it by (`response::hoist`, formed by
/// [`octocode_runtime_base::response::symbol_outline::outline_lines`], the
/// function this prices through), which is the form every reply carries.
/// A declaration that would not form a string is priced as the object it
/// would stay.
pub(super) fn listed_chars(declaration: &super::types::Enclosing) -> usize {
    use octocode_engine::json_size::utf16_units_or;
    let object = || utf16_units_or(declaration, usize::MAX);
    let Ok(row) = serde_json::to_value(declaration) else {
        return object();
    };
    let lines =
        octocode_runtime_base::response::symbol_outline::outline_lines(std::slice::from_ref(&row));
    match lines.as_slice() {
        [line] => utf16_units_or(line, usize::MAX),
        _ => object(),
    }
    .saturating_add(1)
}

/// What a page cut reserves per hit row of a file it has not outlined (a
/// later page's, or one it could not read; see
/// [`super::layout::NamePricing`]) for its `enclosing` name: the field and
/// one listed declaration ([`listed_chars`]) of a typical size, a
/// `function` with four-digit lines and a name of
/// [`TYPICAL_NAME_CHARS`]. A longer name still goes on its row; the page
/// runs that much over budget.
pub(super) static ENCLOSING_RESERVE_CHARS: std::sync::LazyLock<usize> =
    std::sync::LazyLock::new(|| {
        ENCLOSING_FIELD_CHARS
            + listed_chars(&super::types::Enclosing {
                symbol_name: "n".repeat(TYPICAL_NAME_CHARS),
                kind: "function".to_owned(),
                line: 1000,
                end_line: 1000,
            })
    });

/// The symbol name length [`ENCLOSING_RESERVE_CHARS`] reserves for.
const TYPICAL_NAME_CHARS: usize = 18;

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
    let limit = octocode_runtime_base::security::SourceLimits::REAL.whole_source;
    let bytes = super::sources::read_entry(source, limit)?;
    if let super::sources::Expected::Stored(stored) = expected
        && stored != Some(octocode_engine::digest::sha256_digest(&bytes))
    {
        return None;
    }
    Some(bytes)
}

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
    let digest = octocode_engine::digest::sha256(bytes);
    let path = source.to_string_lossy();
    if let Some(kept) = OUTLINES.get(&digest, &path, |kept| Outline::clone(kept)) {
        return Some(kept);
    }
    let limit = octocode_runtime_base::security::SourceLimits::REAL.whole_source;
    let text = security.decode_source_bytes(bytes, limit).ok()?;
    let outline = std::sync::Arc::new(super::enclosing::Outline::of(&text, &path)?);
    OUTLINES.put(
        digest,
        path.into_owned(),
        outline.weight(),
        std::sync::Arc::clone(&outline),
    );
    Some(Outline::clone(&outline))
}

/// Lines a shown row stands for: every hit line of a merged block, or every
/// line a match spanning several covers.
pub(super) fn row_lines(row: &SearchMatch) -> Vec<u32> {
    match (&row.match_lines, row.end_line) {
        (Some(lines), _) => lines.clone(),
        (None, Some(last)) => (row.line..=last.max(row.line)).collect(),
        (None, None) => vec![row.line],
    }
}

/// Name the enclosing declaration of every hit row of a page's files, and
/// the declaration a hit declares. A file lists each declaration once and a
/// hit names them by index.
/// Each file is parsed once. The page cut charged the names' size, so a
/// name is never traded for budget; rows of a file over 1 MiB are counted
/// in the returned note, never dropped silently.
pub(super) fn annotate_enclosing(
    files: &mut [SearchFile],
    declaring: Option<&str>,
    output_root: &std::path::Path,
    expected: &dyn Fn(&std::path::Path) -> super::sources::Expected,
    mut outlines: super::verify::Outlines,
    security: &ContentSecurity,
) -> Option<String> {
    let mut names = Vec::with_capacity(files.len());
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
        names.push(
            row_owners(&outline, matches, declaring)
                .into_iter()
                .zip(row_declared(&outline, matches))
                .collect(),
        );
    }
    for (file, names) in files.iter_mut().zip(names) {
        let mut declarations = Vec::new();
        for (row, (owners, declared)) in file.matches.iter_mut().flatten().zip(names) {
            row.enclosing = owners_of(&mut declarations, owners);
            row.declaration = declared_of(&mut declarations, declared);
        }
        file.declarations = declarations;
    }

    (oversized_rows > 0).then(|| {
        format!(
            "Enclosing declarations (enclosing) and the ones hits declare (declaration) are not named in files over 1 MiB; {} there {} none.",
            hit_rows(oversized_rows),
            have(oversized_rows)
        )
    })
}

/// `1 hit row`, `3 hit rows`.
fn hit_rows(rows: usize) -> String {
    format!("{rows} hit {}", if rows == 1 { "row" } else { "rows" })
}

pub(super) fn have(rows: usize) -> &'static str {
    if rows == 1 { "has" } else { "have" }
}

/// Whether `source`'s language has a declaration outline (by extension),
/// or it is a Markdown file, outlined by its sections.
pub(super) fn outlined_language(source: &std::path::Path) -> bool {
    source
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            octocode_engine::signatures::languages::find_entry(&extension.to_ascii_lowercase())
                .is_some()
        })
        || octocode_tools_shared::source_view::sections::sectioned(&source.to_string_lossy())
}

/// Each row's enclosing declaration, on every row inside one: so no row
/// reads as having none. With `declaring` (a declaration search), only rows that declare
/// that symbol are named.
/// A row's `enclosing`, from the owner of each of its hits (one per
/// matched line of a merged window), each owner listed in `declarations`
/// after the declarations holding it (its chain, the owner last):
/// one index when its hits share one, an index per hit when every hit has
/// one and they differ. A window of which only some hits sit in a
/// declaration names its first hit's, as a one-hit row does, and lists
/// the others.
fn owners_of(
    declarations: &mut Vec<super::types::Enclosing>,
    owners: Vec<Option<Vec<super::types::Enclosing>>>,
) -> Option<super::types::Owners> {
    use super::types::Owners;
    let indexes = owners
        .into_iter()
        .map(|chain| {
            chain?
                .into_iter()
                .map(|declaration| index_of(declarations, declaration))
                .last()
        })
        .collect::<Vec<_>>();
    let first = *indexes.first()?;
    if indexes.iter().all(|index| *index == first) {
        return first.map(Owners::One);
    }
    match indexes.iter().copied().collect::<Option<Vec<_>>>() {
        Some(every) => Some(Owners::PerLine(every)),
        None => first.map(Owners::One),
    }
}

/// `name`'s index in `declarations`, listing it there the first time.
fn index_of(declarations: &mut Vec<super::types::Enclosing>, name: super::types::Enclosing) -> u32 {
    let index = declarations
        .iter()
        .position(|known| *known == name)
        .unwrap_or_else(|| {
            declarations.push(name);
            declarations.len() - 1
        });
    octocode_engine::num::saturating_u32(index)
}

/// A row's `declaration`, from the declarations its hits declare, each
/// listed in `declarations`: one index, or one per declaring hit, in
/// order.
fn declared_of(
    declarations: &mut Vec<super::types::Enclosing>,
    declared: Vec<super::types::Enclosing>,
) -> Option<super::types::Declared> {
    use super::types::Declared;
    let mut indexes = declared
        .into_iter()
        .map(|declaration| index_of(declarations, declaration))
        .collect::<Vec<_>>();
    indexes.dedup();
    match indexes.as_slice() {
        [] => None,
        [one] => Some(Declared::One(*one)),
        _ => Some(Declared::Each(indexes)),
    }
}

/// The declarations each row's hits declare ([`Outline::declared`]), in
/// hit order.
pub(super) fn row_declared(
    outline: &super::enclosing::Outline,
    rows: &[SearchMatch],
) -> Vec<Vec<super::types::Enclosing>> {
    rows.iter()
        .map(|row| {
            hits_of(row)
                .filter_map(|(line, (byte, len))| outline.declared(line, byte, len))
                .filter_map(|index| outline.label(index))
                .collect()
        })
        .collect()
}

pub(super) fn row_owners(
    outline: &super::enclosing::Outline,
    rows: &[SearchMatch],
    declaring: Option<&str>,
) -> Vec<Vec<Option<Vec<super::types::Enclosing>>>> {
    rows.iter()
        .map(|row| {
            let named = declaring.is_none_or(|symbol| {
                row_lines(row)
                    .iter()
                    .any(|line| outline.declares(symbol, *line))
            });
            if !named {
                return vec![None];
            }
            hits_of(row)
                .map(|(line, (byte, _))| {
                    outline
                        .owner(line, byte)
                        .map(|owner| outline.chain(owner))
                        .filter(|chain| !chain.is_empty())
                })
                .collect()
        })
        .collect()
}

/// Each hit of `row`, by line and the bytes of it the hit covers (where it
/// starts, and how many): every hit a merged window holds, or the row's
/// one hit (a match spanning lines starts on its first).
fn hits_of(row: &SearchMatch) -> impl Iterator<Item = (u32, (u32, u32))> + '_ {
    let merged = row
        .match_lines
        .as_deref()
        .filter(|lines| lines.len() == row.match_spans.len() && lines.len() > 1);
    let one = merged
        .is_none()
        .then_some((row.line, (row.byte_offset, row.match_len)));
    merged
        .into_iter()
        .flat_map(|lines| lines.iter().copied().zip(row.match_spans.iter().copied()))
        .chain(one)
}

#[cfg(test)]
mod tests;
