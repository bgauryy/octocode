//! The opt-in SCIP precise tier for Rust call edges: a SCIP index read once,
//! its symbols mapped to graph declarations, and a lookup of the symbol a
//! call site's name token references. The call linker (`tables/resolve`)
//! asks [`ScipIndex::call_target`]; an answer replaces the syntax link, no
//! answer leaves it as it was.
//!
//! Flow: [`ScipIndex::parse`] (pure bytes) then, per file, [`ScipIndex::bind_file`]
//! with the file's text and its declarations (columns become UTF-8 byte
//! offsets; each definition occurrence maps its symbol to one declaration).

mod reader;
#[cfg(test)]
pub(crate) mod tests;

use reader::PositionEncoding;
pub(crate) use reader::ScipError;
use std::collections::HashMap;

/// `SymbolRole.Definition`.
const ROLE_DEFINITION: u32 = 1;

/// What the projection needs to bring a SCIP index into a build: the index
/// (bound in place) and each graph file's text by graph path.
pub(crate) struct ScipInput<'a> {
    pub index: &'a mut ScipIndex,
    pub text: &'a dyn Fn(&str) -> Option<String>,
}

/// A graph declaration a definition occurrence can name.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScipDecl<'a> {
    /// The graph node of the declaration.
    pub node: u32,
    pub name: &'a str,
    /// One-based line of its name, as the graph numbers lines.
    pub line: u32,
    /// Byte range of its name token in the file, when the facts have it.
    pub selection: Option<(u32, u32)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Target {
    /// No definition seen in a bound file.
    Unmapped,
    Node(u32),
    /// Definitions that name different declarations, or one that names none.
    Ambiguous,
}

/// A global SCIP symbol and the declaration its definition maps to.
#[derive(Debug)]
pub(crate) struct ScipSymbol {
    #[cfg(test)]
    symbol: Box<str>,
    target: Target,
}

impl ScipSymbol {
    #[cfg(test)]
    pub(crate) fn symbol(&self) -> &str {
        &self.symbol
    }

    /// The one graph declaration this symbol is defined as; `None` when no
    /// definition was bound or the definitions disagree.
    pub(crate) fn declaration(&self) -> Option<u32> {
        match self.target {
            Target::Node(node) => Some(node),
            Target::Unmapped | Target::Ambiguous => None,
        }
    }

    fn merge(&mut self, found: Option<u32>) {
        self.target = match (self.target, found) {
            (Target::Ambiguous, _) | (_, None) => Target::Ambiguous,
            (Target::Unmapped, Some(node)) => Target::Node(node),
            (Target::Node(known), Some(node)) if known == node => Target::Node(known),
            (Target::Node(_), Some(_)) => Target::Ambiguous,
        };
    }
}

#[derive(Clone, Copy, Debug)]
struct Occurrence {
    symbol: u32,
    definition: bool,
    /// Zero-based start line; columns count the document's encoding until
    /// [`ScipIndex::bind_file`] rewrites them as UTF-8 bytes.
    line: u32,
    start_col: u32,
    end_line: u32,
    end_col: u32,
    /// Byte offsets in the file text; set by `bind_file`.
    start: u32,
    end: u32,
}

struct Document {
    encoding: PositionEncoding,
    occurrences: Vec<Occurrence>,
    /// The file text, once bound.
    text: Option<String>,
}

/// A parsed SCIP index of global symbols.
pub(crate) struct ScipIndex {
    documents: HashMap<String, Document>,
    symbols: Vec<ScipSymbol>,
}

impl ScipIndex {
    /// Reads a serialized SCIP `Index`. `local ` symbols are dropped;
    /// definitions are occurrences whose roles hold the definition bit.
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, ScipError> {
        let mut symbols = Vec::<ScipSymbol>::new();
        let mut ids = HashMap::<&str, u32>::new();
        let mut documents = HashMap::<String, Document>::new();
        for raw in reader::documents(bytes)? {
            let mut occurrences = Vec::with_capacity(raw.occurrences.len());
            for occurrence in raw.occurrences {
                if occurrence.symbol.is_empty() || occurrence.symbol.starts_with("local ") {
                    continue;
                }
                let next = u32::try_from(symbols.len()).unwrap_or(u32::MAX);
                let symbol = *ids.entry(occurrence.symbol).or_insert_with(|| {
                    symbols.push(ScipSymbol {
                        #[cfg(test)]
                        symbol: occurrence.symbol.into(),
                        target: Target::Unmapped,
                    });
                    next
                });
                occurrences.push(Occurrence {
                    symbol,
                    definition: occurrence.roles & ROLE_DEFINITION != 0,
                    line: occurrence.start_line,
                    start_col: occurrence.start_col,
                    end_line: occurrence.end_line,
                    end_col: occurrence.end_col,
                    start: 0,
                    end: 0,
                });
            }
            // A path repeated across documents merges, as SCIP allows.
            let document = documents.entry(raw.path.to_owned()).or_insert(Document {
                encoding: raw.encoding,
                occurrences: Vec::new(),
                text: None,
            });
            document.occurrences.extend(occurrences);
        }
        Ok(Self { documents, symbols })
    }

    pub(crate) fn has_document(&self, file: &str) -> bool {
        self.documents.contains_key(file)
    }

    /// Binds `file`'s text: converts its occurrence columns to UTF-8 bytes
    /// (an occurrence outside the text is dropped) and maps the symbols it
    /// defines to `decls`. A definition names the declaration whose
    /// selection starts at it and whose name it spells, else the one
    /// declaration of that name on its line; none or several make its symbol
    /// ambiguous. Returns whether the file has a document.
    pub(crate) fn bind_file(&mut self, file: &str, text: &str, decls: &[ScipDecl<'_>]) -> bool {
        let Some(document) = self.documents.get_mut(file) else {
            return false;
        };
        let lines = line_starts(text);
        let encoding = document.encoding;
        let convert = |line: u32, col: u32| -> Option<(u32, u32)> {
            let start = *lines.get(line as usize)?;
            let end = lines.get(line as usize + 1).copied().unwrap_or(text.len());
            let row = text.get(start..end)?;
            let in_row = match encoding {
                PositionEncoding::Utf8 => Some(col as usize).filter(|c| row.is_char_boundary(*c)),
                PositionEncoding::Utf16 => units(row, col, char::len_utf16),
                PositionEncoding::Utf32 => units(row, col, |_| 1),
            }
            .filter(|offset| *offset <= row.len())?;
            Some((
                u32::try_from(in_row).ok()?,
                u32::try_from(start + in_row).ok()?,
            ))
        };
        let mut bound = Vec::with_capacity(document.occurrences.len());
        for mut occurrence in std::mem::take(&mut document.occurrences) {
            let (Some((start_col, start)), Some((end_col, end))) = (
                convert(occurrence.line, occurrence.start_col),
                convert(occurrence.end_line, occurrence.end_col),
            ) else {
                continue;
            };
            occurrence.start_col = start_col;
            occurrence.end_col = end_col;
            occurrence.start = start;
            occurrence.end = end;
            bound.push(occurrence);
        }
        bound.sort_by_key(|o| (o.start, o.end, o.symbol));
        let definitions = bound
            .iter()
            .filter(|o| o.definition)
            .map(|o| (o.symbol, declaration_of(o, text, decls)))
            .collect::<Vec<_>>();
        document.occurrences = bound;
        document.text = Some(text.to_owned());
        for (symbol, node) in definitions {
            self.symbols[symbol as usize].merge(node);
        }
        true
    }

    /// The symbol a reference occurrence exactly covers on one line:
    /// `[utf8_col_start, utf8_col_end)` of zero-based `line0`. Definitions
    /// are not references. `None` for an unbound file or no such occurrence.
    /// Exact-token probe: the linker uses [`Self::call_target`].
    #[cfg(test)]
    pub(crate) fn target_at(
        &self,
        file: &str,
        line0: u32,
        utf8_col_start: u32,
        utf8_col_end: u32,
    ) -> Option<&ScipSymbol> {
        let document = self.documents.get(file)?;
        document.text.as_ref()?;
        let found = document.occurrences.iter().find(|o| {
            !o.definition
                && o.line == line0
                && o.end_line == line0
                && o.start_col == utf8_col_start
                && o.end_col == utf8_col_end
        })?;
        Some(&self.symbols[found.symbol as usize])
    }

    /// The graph declaration the call whose expression spans `span` (file
    /// bytes) on zero-based `line0` calls, by the name token it cites: the
    /// first reference occurrence in the span, on that line, that spells
    /// `name` and is followed by `(` or a turbofish. It must carry exactly
    /// one symbol, and that symbol must map to exactly one declaration.
    pub(crate) fn call_target(
        &self,
        file: &str,
        line0: u32,
        span: (u32, u32),
        name: &str,
    ) -> Option<u32> {
        let document = self.documents.get(file)?;
        let text = document.text.as_deref()?;
        let first = document.occurrences.partition_point(|o| o.start < span.0);
        let mut found: Option<&Occurrence> = None;
        for occurrence in &document.occurrences[first..] {
            if occurrence.start >= span.1 || found.is_some_and(|f| f.start != occurrence.start) {
                break;
            }
            let calls_name = !occurrence.definition
                && occurrence.line == line0
                && occurrence.end_line == line0
                && occurrence.end <= span.1
                && text.get(occurrence.start as usize..occurrence.end as usize) == Some(name)
                && text.get(occurrence.end as usize..).is_some_and(is_called);
            if !calls_name {
                continue;
            }
            match found {
                Some(first) if first.symbol != occurrence.symbol => return None,
                Some(_) => {}
                None => found = Some(occurrence),
            }
        }
        self.symbols[found?.symbol as usize].declaration()
    }
}

/// Byte offset of each line's start.
fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect()
}

/// Byte offset of the `col`th unit of `row`, `width` units per char.
fn units(row: &str, col: u32, width: impl Fn(char) -> usize) -> Option<usize> {
    let mut seen = 0usize;
    for (at, ch) in row.char_indices() {
        if seen == col as usize {
            return Some(at);
        }
        seen += width(ch);
        if seen > col as usize {
            return None;
        }
    }
    (seen == col as usize).then_some(row.len())
}

/// Whether the text after a name calls it: `(`, or a turbofish `::<`.
fn is_called(rest: &str) -> bool {
    let rest = rest.trim_start_matches([' ', '\t']);
    rest.starts_with('(') || rest.starts_with("::<")
}

/// The one declaration a definition occurrence names.
fn declaration_of(definition: &Occurrence, text: &str, decls: &[ScipDecl<'_>]) -> Option<u32> {
    let spelled = text.get(definition.start as usize..definition.end as usize)?;
    let unique = |mut nodes: Vec<u32>| {
        nodes.sort_unstable();
        nodes.dedup();
        match nodes[..] {
            [only] => Some(Some(only)),
            [] => None,
            _ => Some(None),
        }
    };
    let at_selection = decls
        .iter()
        .filter(|d| {
            d.name == spelled
                && d.selection
                    .is_some_and(|(start, _)| start == definition.start)
        })
        .map(|d| d.node)
        .collect::<Vec<_>>();
    if let Some(answer) = unique(at_selection) {
        return answer;
    }
    let on_line = decls
        .iter()
        .filter(|d| d.name == spelled && d.line == definition.line + 1)
        .map(|d| d.node)
        .collect::<Vec<_>>();
    unique(on_line).flatten()
}
