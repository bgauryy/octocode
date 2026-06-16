// ── Markdown ─────────────────────────────────────────────────────────────────

pub fn minify_markdown_core(content: &str) -> String {
    let normalized = content.replace("\r\n", "\n");
    let source: Vec<&str> = normalized.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(source.len());
    let mut fence: Option<FenceState> = None;
    let mut in_html_comment = false;
    let mut in_generated_toc = false;
    let src = &source;
    let first_content = src.iter().position(|l| !l.trim().is_empty()).unwrap_or(0);
    let mut in_frontmatter =
        first_content == 0 && src.first().map(|l| l.trim_end() == "---").unwrap_or(false);

    let mut i = 0;
    while i < src.len() {
        let original = src[i];

        // Inside code fence
        if let Some(ref f) = fence {
            append_md(&mut out, original, true);
            if is_fence_close(original, f) {
                fence = None;
            }
            i += 1;
            continue;
        }
        // Fence start
        if let Some(f) = fence_start(original) {
            fence = Some(f);
            append_md(&mut out, original.trim_end(), true);
            i += 1;
            continue;
        }
        // Indented code
        if original.starts_with("    ") || original.starts_with('\t') {
            append_md(&mut out, original, true);
            i += 1;
            continue;
        }
        // Frontmatter
        if in_frontmatter {
            let cleaned = strip_md_inline_noise(original.trim_end());
            append_md(&mut out, &cleaned, false);
            if i > 0 && (original.trim_end() == "---" || original.trim_end() == "...") {
                in_frontmatter = false;
            }
            i += 1;
            continue;
        }
        // Generated TOC
        if in_generated_toc {
            if is_toc_end(original) {
                in_generated_toc = false;
                append_md(&mut out, "", false);
            }
            i += 1;
            continue;
        }
        if is_toc_start(original) {
            in_generated_toc = !is_toc_end(original);
            append_md(&mut out, "", false);
            i += 1;
            continue;
        }
        // Strip HTML comments
        let (stripped, still_in_comment) = strip_md_html_comment(original, in_html_comment);
        in_html_comment = still_in_comment;
        let line = &stripped;
        // Pseudo-comment or badge line
        if is_pseudo_comment(line) || is_discardable_md_line(line) {
            append_md(&mut out, "", false);
            i += 1;
            continue;
        }
        // Setext heading conversion
        if let Some(level) = setext_level(line) {
            if convert_setext(&mut out, level) {
                i += 1;
                continue;
            }
        }
        // Thematic break
        if is_thematic_break(line) {
            append_md(&mut out, "---", false);
            i += 1;
            continue;
        }
        // Table row
        let is_table = is_delimiter_row(line)
            || src
                .get(i.saturating_sub(1))
                .is_some_and(|l| is_delimiter_row(l))
            || src.get(i + 1).is_some_and(|l| is_delimiter_row(l));
        let compacted = if is_table {
            compact_table_row(line.trim_end())
        } else {
            compact_md_line(line)
        };
        append_md(&mut out, &compacted, false);
        i += 1;
    }
    let compacted = compact_markdown_newlines(out);
    let joined = compacted.join("\n");
    joined.trim().to_owned()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

struct FenceState {
    marker: char,
    length: usize,
}

fn fence_start(line: &str) -> Option<FenceState> {
    let leading = line.len() - line.trim_start().len();
    if leading > 3 {
        return None;
    }
    let rest = line.trim_start();
    let marker = rest.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let length = rest.chars().take_while(|&c| c == marker).count();
    if length >= 3 {
        Some(FenceState { marker, length })
    } else {
        None
    }
}

fn is_fence_close(line: &str, f: &FenceState) -> bool {
    let rest = line.trim_start();
    let count = rest.chars().take_while(|&c| c == f.marker).count();
    if count < f.length {
        return false;
    }
    rest[count..].trim().is_empty()
}

fn is_thematic_break(line: &str) -> bool {
    let compact: String = line.trim().chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() < 3 {
        return false;
    }
    let Some(m) = compact.chars().next() else {
        return false;
    };
    (m == '-' || m == '_' || m == '*') && compact.chars().all(|c| c == m)
}

fn setext_level(line: &str) -> Option<u8> {
    let t = line.trim();
    if t.chars().all(|c| c == '=') && !t.is_empty() {
        Some(1)
    } else if t.chars().all(|c| c == '-') && !t.is_empty() {
        Some(2)
    } else {
        None
    }
}

fn convert_setext(out: &mut Vec<String>, level: u8) -> bool {
    let prefix = if level == 1 { "# " } else { "## " };
    let mut heading_lines: Vec<String> = Vec::new();
    while let Some(l) = out.last() {
        if l.trim().is_empty() {
            break;
        }
        if let Some(line) = out.pop() {
            heading_lines.push(line);
        }
    }
    if heading_lines.is_empty() {
        return false;
    }
    heading_lines.reverse();
    let text = heading_lines
        .iter()
        .map(|l| l.trim().to_owned())
        .collect::<Vec<_>>()
        .join(" ");
    let candidate = format!("{}{}", prefix, text);
    append_md(out, &candidate, false);
    true
}

fn is_toc_start(line: &str) -> bool {
    regex_like_toc(line).0
}
fn is_toc_end(line: &str) -> bool {
    regex_like_toc(line).1
}
fn regex_like_toc(line: &str) -> (bool, bool) {
    let lower = line.to_lowercase();
    let is_end =
        lower.contains("<!-- end") || lower.contains("<!-- /toc") || lower.contains("tocstop");
    let is_start = !is_end
        && (lower.contains("<!-- toc")
            || lower.contains("<!-- table of contents")
            || lower.contains("<!-- doctoc")
            || lower.contains("<!-- markdown-toc"));
    (is_start, is_end)
}

fn strip_md_html_comment(line: &str, mut in_comment: bool) -> (String, bool) {
    let mut output = String::new();
    let mut cur = 0usize;
    let bytes = line.as_bytes();
    while cur < bytes.len() {
        if in_comment {
            if let Some(pos) = &line[cur..].find("-->") {
                cur += pos + 3;
                in_comment = false;
            } else {
                return (output, true);
            }
        } else {
            if let Some(pos) = line[cur..].find("<!--") {
                output.push_str(&line[cur..cur + pos]);
                let rest = &line[cur + pos..];
                if let Some(end) = rest.find("-->") {
                    cur += pos + end + 3;
                } else {
                    return (output, true);
                }
            } else {
                output.push_str(&line[cur..]);
                break;
            }
        }
    }
    (output, in_comment)
}

fn is_pseudo_comment(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("[//]: #")
}

fn is_discardable_md_line(line: &str) -> bool {
    is_badge_line(line)
        || is_image_only_line(line)
        || is_anchor_only_line(line)
        || is_html_break_only_line(line)
}

fn is_badge_line(line: &str) -> bool {
    let t = line.trim();
    let badge_domains = [
        "img.shields.io",
        "badge.fury.io",
        "badgen.net",
        "codecov.io",
        "coveralls.io",
        "circleci.com",
        "travis-ci",
    ];
    let images: Vec<_> = t.match_indices("![").collect();
    if images.is_empty() {
        return false;
    }
    let mut has_badge = false;
    for (_, _) in &images {
        let all_badges = badge_domains.iter().any(|d| t.contains(d));
        if all_badges {
            has_badge = true;
        }
    }
    has_badge && {
        let mut cleaned = t.to_owned();
        while let Some(start) = cleaned.find("![") {
            if let Some(end) = cleaned[start..].find(')') {
                cleaned.replace_range(start..start + end + 1, "");
            } else {
                break;
            }
        }
        cleaned.trim().is_empty()
    }
}

fn is_image_only_line(line: &str) -> bool {
    let mut cleaned = line.trim().to_owned();
    loop {
        let next = strip_one_markdown_image(&cleaned);
        if next == cleaned {
            break;
        }
        cleaned = next;
    }
    cleaned.trim().is_empty()
}

fn strip_one_markdown_image(line: &str) -> String {
    if let Some(start) = line.find("[![") {
        if let Some(end) = linked_image_end(line, start) {
            let mut output = String::with_capacity(line.len());
            output.push_str(&line[..start]);
            output.push_str(&line[end..]);
            return output;
        }
    }
    if let Some(start) = line.find("![") {
        if let Some(end) = markdown_image_end(line, start) {
            let mut output = String::with_capacity(line.len());
            output.push_str(&line[..start]);
            output.push_str(&line[end..]);
            return output;
        }
    }
    line.to_owned()
}

fn markdown_image_end(line: &str, start: usize) -> Option<usize> {
    let label_end = line[start + 2..].find(']')? + start + 2;
    let rest = &line[label_end + 1..];
    if !rest.starts_with('(') {
        return None;
    }
    let target_end = rest.find(')')?;
    Some(label_end + 1 + target_end + 1)
}

fn linked_image_end(line: &str, start: usize) -> Option<usize> {
    let image_end = markdown_image_end(line, start + 1)?;
    let rest = &line[image_end..];
    if !rest.starts_with("](") {
        return None;
    }
    let link_end = rest[2..].find(')')?;
    Some(image_end + 2 + link_end + 1)
}

fn is_anchor_only_line(line: &str) -> bool {
    let t = line.trim().to_ascii_lowercase();
    if t.is_empty() {
        return false;
    }
    (t.starts_with("<a ") || t.starts_with("<a\t"))
        && t.contains("id=")
        && (t.ends_with("</a>") || t.ends_with("/>"))
}

fn is_html_break_only_line(line: &str) -> bool {
    matches!(
        line.trim().to_ascii_lowercase().as_str(),
        "<br>" | "<br/>" | "<br />"
    )
}

fn is_delimiter_row(line: &str) -> bool {
    let parts: Vec<&str> = line.trim().split('|').filter(|p| !p.is_empty()).collect();
    parts.len() >= 2
        && parts.iter().all(|p| {
            let t = p.trim();
            t.starts_with(':')
                || t.ends_with(':')
                || t.trim_matches('-').trim_matches(':').is_empty()
        })
}

fn compact_table_row(line: &str) -> String {
    let compacted = line
        .split('|')
        .map(|p| p.trim())
        .collect::<Vec<_>>()
        .join("|");
    strip_md_inline_noise(&compacted)
}

fn compact_md_line(line: &str) -> String {
    let cleaned = strip_md_inline_noise(line);
    let s = cleaned.trim_end();
    let mut result = s.to_owned();
    // Compact ATX heading
    if result.starts_with('#') {
        let hash_count = result.chars().take_while(|&c| c == '#').count().min(6);
        let text = result[hash_count..].trim().trim_end_matches('#').trim();
        if !text.is_empty() {
            result = format!("{} {}", "#".repeat(hash_count), text);
        }
    }
    result
}

fn strip_md_inline_noise(line: &str) -> String {
    let without_images = strip_inline_markdown_images(line);
    let without_emoji_shortcodes = strip_md_emoji_shortcodes(&without_images);
    let without_emoji = strip_unicode_emoji(&without_emoji_shortcodes);
    let without_breaks = strip_inline_html_breaks(&without_emoji);
    normalize_md_inline_spacing(&without_breaks)
}

fn strip_inline_markdown_images(line: &str) -> String {
    let mut cleaned = line.to_owned();
    loop {
        let next = strip_one_markdown_image(&cleaned);
        if next == cleaned {
            break;
        }
        cleaned = next;
    }
    cleaned
}

fn strip_md_emoji_shortcodes(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut i = 0usize;
    while i < line.len() {
        if line.as_bytes()[i] == b':' {
            if let Some(end_offset) = line[i + 1..].find(':') {
                let end = i + 1 + end_offset;
                let label = &line[i + 1..end];
                if is_emoji_shortcode_label(label) {
                    i = end + 1;
                    continue;
                }
            }
        }
        i = super::copy_seq(line, i, &mut out);
    }
    out
}

fn is_emoji_shortcode_label(label: &str) -> bool {
    let len = label.len();
    (2..=40).contains(&len)
        && label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'+'))
}

fn strip_unicode_emoji(line: &str) -> String {
    line.chars().filter(|&ch| !is_emoji_scalar(ch)).collect()
}

fn is_emoji_scalar(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1F000..=0x1FAFF
            | 0x1FB00..=0x1FFFF
            | 0x2600..=0x27BF
            | 0xFE00..=0xFE0F
            | 0x200D
            | 0x20E3
    )
}

fn strip_inline_html_breaks(line: &str) -> String {
    line.replace("<br />", " ")
        .replace("<br/>", " ")
        .replace("<br>", " ")
        .replace("<BR />", " ")
        .replace("<BR/>", " ")
        .replace("<BR>", " ")
}

fn normalize_md_inline_spacing(line: &str) -> String {
    let compact = line.split_whitespace().collect::<Vec<_>>().join(" ");
    compact
        .replace(" .", ".")
        .replace(" ,", ",")
        .replace(" ;", ";")
        .replace(" :", ":")
        .replace(" !", "!")
        .replace(" ?", "?")
        .trim()
        .to_owned()
}

fn compact_markdown_newlines(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut fence: Option<FenceState> = None;

    for line in lines {
        if let Some(ref active_fence) = fence {
            if is_fence_close(&line, active_fence) {
                fence = None;
            }
            out.push(line);
            continue;
        }

        if let Some(next_fence) = fence_start(&line) {
            fence = Some(next_fence);
            out.push(line);
            continue;
        }

        if line.trim().is_empty() {
            continue;
        }

        if let Some(prev) = out.last_mut() {
            if is_markdown_paragraph_line(prev) && is_markdown_paragraph_line(&line) {
                prev.push(' ');
                prev.push_str(line.trim());
                continue;
            }
        }

        out.push(line);
    }
    out
}

fn is_markdown_paragraph_line(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return false;
    }
    if fence_start(t).is_some()
        || t.starts_with('#')
        || t.starts_with('>')
        || t.starts_with('<')
        || t.starts_with('[') && t.contains("]:")
        || t.contains('|')
        || is_thematic_break(t)
        || is_delimiter_row(t)
        || is_markdown_list_item(t)
    {
        return false;
    }
    true
}

fn is_markdown_list_item(line: &str) -> bool {
    let t = line.trim_start();
    if t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ") {
        return true;
    }

    let mut saw_digit = false;
    for (idx, ch) in t.char_indices() {
        if ch.is_ascii_digit() {
            saw_digit = true;
            continue;
        }
        return saw_digit
            && matches!(ch, '.' | ')')
            && t[idx + ch.len_utf8()..].starts_with(char::is_whitespace);
    }
    false
}

fn append_md(out: &mut Vec<String>, line: &str, preserve_blank: bool) {
    if line.trim().is_empty() {
        if preserve_blank || out.last().is_some_and(|l| !l.trim().is_empty()) {
            out.push(String::new());
        }
    } else {
        out.push(line.to_owned());
    }
}
