pub(crate) fn markdown_heading_outline(content: &str, path: &str) -> Option<String> {
    let path = path
        .split(['?', '#'])
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase();
    if ![".md", ".markdown", ".mdx"]
        .iter()
        .any(|ext| path.ends_with(ext))
    {
        return None;
    }
    let mut output = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim_start_matches([' ', '\t']);
        if line.len() - trimmed.len() <= 3 {
            let marker = trimmed.chars().next();
            if matches!(marker, Some('`' | '~')) {
                let count = trimmed.chars().take_while(|c| Some(*c) == marker).count();
                if count >= 3 {
                    if fence.is_none() {
                        fence = Some((marker.unwrap_or('`'), count));
                    } else if fence.is_some_and(|(m, n)| Some(m) == marker && count >= n) {
                        fence = None;
                    }
                    continue;
                }
            }
        }
        if fence.is_some() {
            continue;
        }
        let level = trimmed.chars().take_while(|c| *c == '#').count();
        if !(1..=6).contains(&level) {
            continue;
        }
        let rest = &trimmed[level..];
        if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
            continue;
        }
        let mut text = rest.trim().trim_end_matches('#').trim_end().to_owned();
        if text.is_empty() {
            text = "(untitled heading)".into();
        }
        output.push(format!(
            "{:>4}| {}{} {}",
            index + 1,
            "  ".repeat(level - 1),
            "#".repeat(level),
            text
        ));
    }
    (!output.is_empty()).then(|| output.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn headings_skip_fences_and_preserve_hierarchy() {
        let text = "# Root\n```\n## hidden\n```\n  ## Child ###\n";
        assert_eq!(
            markdown_heading_outline(text, "README.md").as_deref(),
            Some("   1| # Root\n   5|   ## Child")
        );
        assert_eq!(markdown_heading_outline(text, "README.txt"), None);
    }
}
