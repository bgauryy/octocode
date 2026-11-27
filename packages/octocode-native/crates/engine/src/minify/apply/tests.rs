use super::*;

/// The indentation the grammar of `file_path` keeps in `content`; `None`
/// when no grammar lexes it.
pub(in crate::minify) fn indent_of(content: &str, file_path: &str) -> Option<Indent> {
    match lex(content, file_path, &|| false).0 {
        Layout::Grammar(_, indent, _) => Some(indent),
        _ => None,
    }
}
