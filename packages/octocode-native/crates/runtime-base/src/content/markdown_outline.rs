/// One ATX heading of a Markdown file: its 1-based line, its level (1–6)
/// and its text (`(untitled heading)` for none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownHeading {
    pub line: usize,
    pub level: usize,
    pub text: String,
}

/// The `symbols` outline of a Markdown file: one row per heading,
/// `line<TAB>` then the heading indented by its level.
pub fn markdown_heading_outline(content: &str, path: &str) -> Option<String> {
    let output = markdown_headings(content, path)?
        .into_iter()
        .map(|heading| {
            format!(
                "{}\t{}{} {}",
                heading.line,
                "  ".repeat(heading.level - 1),
                "#".repeat(heading.level),
                heading.text
            )
        })
        .collect::<Vec<_>>();
    (!output.is_empty()).then(|| output.join("\n"))
}

/// The ATX headings of a Markdown file in line order, none inside a fenced
/// code block; `None` for a path that is no Markdown file.
pub fn markdown_headings(content: &str, path: &str) -> Option<Vec<MarkdownHeading>> {
    let path = path.split(['?', '#']).next().unwrap_or(path);
    if super::DocumentFormat::of(path) != Some(super::DocumentFormat::Markdown) {
        return None;
    }
    let mut output = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim_start_matches(' ');
        if line.len() - trimmed.len() > 3 || trimmed.starts_with('\t') {
            continue;
        }
        if let Some((marker, length)) = fence {
            let count = trimmed.bytes().take_while(|&byte| byte == marker).count();
            if count >= length && trimmed[count..].trim_matches([' ', '\t']).is_empty() {
                fence = None;
            }
            continue;
        }
        if let Some(marker @ (b'`' | b'~')) = trimmed.bytes().next() {
            let count = trimmed.bytes().take_while(|&byte| byte == marker).count();
            if count >= 3 && (marker != b'`' || !trimmed[count..].contains('`')) {
                fence = Some((marker, count));
                continue;
            }
        }
        let level = trimmed.chars().take_while(|c| *c == '#').count();
        if !(1..=6).contains(&level) {
            continue;
        }
        let rest = &trimmed[level..];
        if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
            continue;
        }
        let text = rest.trim_matches([' ', '\t']);
        let closing = text.trim_end_matches('#');
        let text = if closing.len() < text.len()
            && (closing.is_empty() || closing.ends_with([' ', '\t']))
        {
            closing.trim_end_matches([' ', '\t'])
        } else {
            text
        };
        let text = if text.is_empty() {
            "(untitled heading)"
        } else {
            text
        };
        output.push(MarkdownHeading {
            line: index + 1,
            level,
            text: text.to_owned(),
        });
    }
    Some(output)
}

#[cfg(test)]
mod tests;
