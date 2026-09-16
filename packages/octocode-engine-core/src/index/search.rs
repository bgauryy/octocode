use regex::RegexBuilder;

use super::ContentRecord;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IndexQueryKind {
    Content,
    Symbol,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexQuery {
    pub text: String,
    pub kind: IndexQueryKind,
    pub case_sensitive: bool,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexQueryMatch {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub value: String,
    pub symbol_kind: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexQueryResult {
    pub query: IndexQuery,
    pub matches: Vec<IndexQueryMatch>,
    pub total_matches: usize,
    pub next_offset: Option<usize>,
}

#[must_use]
pub fn query_documents(documents: &[ContentRecord], query: &IndexQuery) -> IndexQueryResult {
    let mut ordered = documents.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.path.cmp(&right.path));

    let matcher = RegexBuilder::new(&regex::escape(&query.text))
        .case_insensitive(!query.case_sensitive)
        .build()
        .expect("escaped literal is always a valid regex");
    let mut matches = Vec::new();

    for document in ordered {
        match query.kind {
            IndexQueryKind::Content => {
                for found in matcher.find_iter(&document.content) {
                    let (line, column) = line_column(&document.content, found.start());
                    matches.push(IndexQueryMatch {
                        path: document.path.clone(),
                        line,
                        column,
                        start_byte: found.start(),
                        end_byte: found.end(),
                        value: document.content[found.start()..found.end()].to_owned(),
                        symbol_kind: None,
                    });
                }
            }
            IndexQueryKind::Symbol => {
                for symbol in &document.symbols {
                    let Some(found) = matcher.find(&symbol.name) else {
                        continue;
                    };
                    if found.start() != 0 || found.end() != symbol.name.len() {
                        continue;
                    }
                    let start = usize::try_from(symbol.start_byte)
                        .unwrap_or(usize::MAX)
                        .min(document.content.len());
                    let end = usize::try_from(symbol.end_byte)
                        .unwrap_or(usize::MAX)
                        .min(document.content.len());
                    let (line, column) = line_column(&document.content, start);
                    matches.push(IndexQueryMatch {
                        path: document.path.clone(),
                        line,
                        column,
                        start_byte: start,
                        end_byte: end.max(start),
                        value: symbol.name.clone(),
                        symbol_kind: Some(symbol.kind.clone()),
                    });
                }
            }
        }
    }

    let total_matches = matches.len();
    let offset = query.offset.min(total_matches);
    let limit = query.limit.max(1);
    let end = offset.saturating_add(limit).min(total_matches);
    let page = matches[offset..end].to_vec();
    IndexQueryResult {
        query: query.clone(),
        matches: page,
        total_matches,
        next_offset: (end < total_matches).then_some(end),
    }
}

fn line_column(content: &str, byte_offset: usize) -> (usize, usize) {
    let prefix = &content[..byte_offset.min(content.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix, |(_, tail)| tail)
        .chars()
        .count();
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{ContentRecord, SourceFileIdentity, SymbolRecord};

    fn document(path: &str, content: &str, symbols: Vec<SymbolRecord>) -> ContentRecord {
        ContentRecord {
            path: path.to_owned(),
            language: "rust".to_owned(),
            content: content.to_owned(),
            identity: SourceFileIdentity {
                size: content.len() as u64,
                modified_nanos: 1,
                content_digest: "fixture".to_owned(),
            },
            symbols,
        }
    }

    #[test]
    fn literal_query_is_deterministic_paginated_and_line_aware() {
        let documents = vec![
            document("b.rs", "first\nneedle two\n", vec![]),
            document("a.rs", "needle one\nneedle three\n", vec![]),
        ];
        let first = query_documents(
            &documents,
            &IndexQuery {
                text: "needle".to_owned(),
                kind: IndexQueryKind::Content,
                case_sensitive: true,
                offset: 0,
                limit: 2,
            },
        );
        assert_eq!(first.total_matches, 3);
        assert_eq!(first.matches.len(), 2);
        assert_eq!(first.matches[0].path, "a.rs");
        assert_eq!(first.matches[0].line, 1);
        assert_eq!(first.matches[1].line, 2);
        assert_eq!(first.next_offset, Some(2));

        let second = query_documents(
            &documents,
            &IndexQuery {
                offset: 2,
                ..first.query.clone()
            },
        );
        assert_eq!(second.matches.len(), 1);
        assert_eq!(second.matches[0].path, "b.rs");
        assert_eq!(second.next_offset, None);
    }

    #[test]
    fn symbol_query_matches_names_without_scanning_source_text() {
        let documents = vec![document(
            "lib.rs",
            "fn hidden() {}\n",
            vec![SymbolRecord {
                name: "VisibleAnswer".to_owned(),
                kind: "function".to_owned(),
                start_byte: 0,
                end_byte: 14,
            }],
        )];
        let result = query_documents(
            &documents,
            &IndexQuery {
                text: "visibleanswer".to_owned(),
                kind: IndexQueryKind::Symbol,
                case_sensitive: false,
                offset: 0,
                limit: 10,
            },
        );
        assert_eq!(result.total_matches, 1);
        assert_eq!(result.matches[0].value, "VisibleAnswer");
        assert_eq!(result.matches[0].symbol_kind.as_deref(), Some("function"));
    }
}
