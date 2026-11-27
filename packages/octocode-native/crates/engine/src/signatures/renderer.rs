use super::languages::CommentStyle;

/// Whether `text` is a whole-line comment in `comments` syntax.
pub(crate) fn is_pure_comment(text: &str, comments: CommentStyle) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    if t.starts_with("#!") {
        return false;
    } // shebang
    match comments {
        CommentStyle::C => {
            if t.starts_with("//") || t.starts_with('*') {
                return true;
            }
            t.starts_with("/*") && t.find("*/").is_none_or(|i| t[i + 2..].trim().is_empty())
        }
        CommentStyle::Hash => t.starts_with('#'),
        CommentStyle::Asm => {
            t.starts_with('#')
                || t.starts_with(';')
                || t.starts_with("//")
                || t.starts_with("/*")
                || t.starts_with('*')
        }
    }
}
