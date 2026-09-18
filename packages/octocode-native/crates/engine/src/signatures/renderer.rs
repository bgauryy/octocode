/// Render `KeptLine`s into the canonical `NNN| text` gutter format.
/// Blank lines and pure-comment lines are excluded before rendering.
pub fn render_skeleton(kept: &[(usize, String)], comment_prefix: &str) -> Option<String> {
    let visible: Vec<&(usize, String)> = kept
        .iter()
        .filter(|(_, text)| !text.trim().is_empty())
        .filter(|(_, text)| !is_pure_comment(text, comment_prefix))
        .collect();

    if visible.is_empty() {
        return None;
    }

    let max_line = visible.iter().map(|(n, _)| *n).max().unwrap_or(1);
    let width = max_line.to_string().len();

    let s = visible
        .iter()
        .map(|(n, text)| format!("{:>width$}| {}", n, text, width = width))
        .collect::<Vec<_>>()
        .join("\n");

    Some(s)
}

fn is_pure_comment(text: &str, prefix: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    if t.starts_with("#!") {
        return false;
    } // shebang
    match prefix {
        "c" | "c-hash" => {
            if t.starts_with("//") {
                return true;
            }
            if t.starts_with('*') {
                return true;
            }
            if t.starts_with("/*") {
                let close = t.find("*/");
                if close.is_none_or(|i| t[i + 2..].trim().is_empty()) {
                    return true;
                }
            }
            if prefix == "c-hash" && t.starts_with('#') {
                return true;
            }
            false
        }
        "hash" => t.starts_with('#'),
        "html" => t.starts_with("<!--") || t.starts_with("-->"),
        "sql" => {
            if t.starts_with("--") {
                return true;
            }
            if t.starts_with("/*") {
                let close = t.find("*/");
                return close.is_none_or(|i| t[i + 2..].trim().is_empty());
            }
            false
        }
        _ => false,
    }
}
