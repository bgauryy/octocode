//! Markdown minification.

mod blocks;
mod inline;
mod paragraphs;

use blocks::{
    FenceState, convert_setext, fence_start, is_discardable_md_line, is_fence_close,
    is_pseudo_comment, is_thematic_break, is_toc_end, is_toc_start, setext_level,
    strip_md_html_comment,
};
use inline::{compact_md_line, compact_table_row, is_delimiter_row, strip_md_inline_noise};
#[cfg(test)]
use paragraphs::is_markdown_paragraph_line;
use paragraphs::{append_md, compact_markdown_newlines};

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
        if let Some(level) = setext_level(line)
            && convert_setext(&mut out, level)
        {
            i += 1;
            continue;
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

#[cfg(test)]
mod tests;
