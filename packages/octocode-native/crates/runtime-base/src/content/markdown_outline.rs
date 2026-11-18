pub fn markdown_heading_outline(content: &str, path: &str) -> Option<String> {
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
        output.push(format!(
            "{}\t{}{} {}",
            index + 1,
            "  ".repeat(level - 1),
            "#".repeat(level),
            text
        ));
    }
    (!output.is_empty()).then(|| output.join("\n"))
}

#[cfg(test)]
mod tests;
