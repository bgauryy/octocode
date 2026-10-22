use super::languages::CommentStyle;

/// Render kept `(line, text)` pairs as numbered source lines
/// (`<line>\t<text>`, the shared numbered-read format).
/// Blank lines and pure-comment lines are excluded before rendering.
pub fn render_skeleton(kept: &[(usize, String)], comments: CommentStyle) -> Option<String> {
    let visible: Vec<&(usize, String)> = kept
        .iter()
        .filter(|(_, text)| !text.trim().is_empty())
        .filter(|(_, text)| !is_pure_comment(text, comments))
        .collect();

    if visible.is_empty() {
        return None;
    }

    // Unpadded numbers: the content-view minifier trims leading whitespace on
    // some lines, which would leave a padded gutter uneven.
    let s = visible
        .iter()
        .map(|(n, text)| format!("{n}\t{text}"))
        .collect::<Vec<_>>()
        .join("\n");

    Some(s)
}

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
