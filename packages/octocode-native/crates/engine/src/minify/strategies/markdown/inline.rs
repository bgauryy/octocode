//! Line-level noise: badges, images, anchors, breaks, emoji and spacing.

pub(super) fn is_badge_line(line: &str) -> bool {
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

pub(super) fn is_image_only_line(line: &str) -> bool {
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
    if let Some(start) = line.find("[![")
        && let Some(end) = linked_image_end(line, start)
    {
        let mut output = String::with_capacity(line.len());
        output.push_str(&line[..start]);
        output.push_str(&line[end..]);
        return output;
    }
    if let Some(start) = line.find("![")
        && let Some(end) = markdown_image_end(line, start)
    {
        let mut output = String::with_capacity(line.len());
        output.push_str(&line[..start]);
        output.push_str(&line[end..]);
        return output;
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

pub(super) fn is_anchor_only_line(line: &str) -> bool {
    let t = line.trim().to_ascii_lowercase();
    if t.is_empty() {
        return false;
    }
    (t.starts_with("<a ") || t.starts_with("<a\t"))
        && t.contains("id=")
        && (t.ends_with("</a>") || t.ends_with("/>"))
}

pub(super) fn is_html_break_only_line(line: &str) -> bool {
    matches!(
        line.trim().to_ascii_lowercase().as_str(),
        "<br>" | "<br/>" | "<br />"
    )
}

pub(super) fn is_delimiter_row(line: &str) -> bool {
    let parts: Vec<&str> = line.trim().split('|').filter(|p| !p.is_empty()).collect();
    parts.len() >= 2
        && parts.iter().all(|p| {
            let t = p.trim();
            t.starts_with(':')
                || t.ends_with(':')
                || t.trim_matches('-').trim_matches(':').is_empty()
        })
}

pub(super) fn compact_table_row(line: &str) -> String {
    let compacted = line.split('|').map(str::trim).collect::<Vec<_>>().join("|");
    strip_md_inline_noise(&compacted)
}

pub(super) fn compact_md_line(line: &str) -> String {
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

pub(super) fn strip_md_inline_noise(line: &str) -> String {
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
        if line.as_bytes()[i] == b':'
            && let Some(end_offset) = line[i + 1..].find(':')
        {
            let end = i + 1 + end_offset;
            let label = &line[i + 1..end];
            if is_emoji_shortcode_label(label) {
                i = end + 1;
                continue;
            }
        }
        i = super::super::copy_seq(line, i, &mut out);
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
