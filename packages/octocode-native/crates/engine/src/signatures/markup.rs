//! The module imports in a markup file's script regions. Vue, Svelte, Astro,
//! MDX and HTML have no grammar and no graph facts, but what their scripts
//! import is code a graph must be able to name.

use std::ops::Range;

/// One module a markup file's script imports or re-exports.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkupImport {
    pub specifier: String,
    /// 1-based, in the markup file's own lines.
    pub line: u32,
    /// The name it takes from that module (`default`, `*`), when it names one.
    pub imported: String,
}

/// What a markup file holds for the graph to name.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MarkupScripts {
    /// It holds script regions no graph node represents.
    pub has_script: bool,
    pub imports: Vec<MarkupImport>,
}

/// The scripts of `content`, a file of markup kind `extension`. The regions
/// parse in place, as the rest of the file blanked to spaces, so every
/// position is the file's own.
#[must_use]
pub fn markup_scripts(content: &str, path: &str, extension: &str) -> MarkupScripts {
    let regions = script_regions(content, extension);
    if regions.is_empty() {
        return MarkupScripts::default();
    }
    let mut scripts = MarkupScripts {
        has_script: true,
        imports: Vec::new(),
    };
    if !regions
        .iter()
        .any(|region| content.get(region.clone()).is_some_and(mentions_import))
    {
        return scripts;
    }
    let Some(facts) = crate::signatures::extract_declarations_document_with_extension(
        &blank_outside(content, &regions),
        path,
        "tsx",
    ) else {
        return scripts;
    };
    scripts
        .imports
        .extend(facts.imports.into_iter().map(|import| MarkupImport {
            specifier: import.specifier,
            line: import.line,
            imported: import.imported_name.unwrap_or_default(),
        }));
    scripts
        .imports
        .extend(facts.exports.into_iter().filter_map(|export| {
            Some(MarkupImport {
                specifier: export.source?,
                line: export.line,
                imported: export.local_name.unwrap_or(export.name),
            })
        }));
    scripts
}

fn mentions_import(text: &str) -> bool {
    text.contains("import") || text.contains("export")
}

/// The script text of a markup file, by kind: `<script>` elements in all of
/// them, and what Astro and MDX write outside one.
fn script_regions(content: &str, extension: &str) -> Vec<Range<usize>> {
    let mut regions = script_elements(content);
    match extension {
        "astro" => regions.extend(frontmatter(content)),
        "mdx" => regions.extend(import_lines(content)),
        _ => {}
    }
    regions.sort_by_key(|region| region.start);
    regions
}

/// The first `needle` (lowercase ASCII) at or after `from`, in any case.
fn find_ascii_lowercase(content: &str, needle: &str, from: usize) -> Option<usize> {
    let tail = content.as_bytes().get(from..)?;
    tail.windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
        .map(|at| from + at)
}

/// The bodies of `<script …>…</script>` elements. An unclosed element runs to
/// the end of the file, as it does in a browser.
fn script_elements(content: &str) -> Vec<Range<usize>> {
    let mut regions = Vec::new();
    let mut from = 0;
    while let Some(open) = find_ascii_lowercase(content, "<script", from) {
        let after_name = open + "<script".len();
        let delimited = content
            .as_bytes()
            .get(after_name)
            .is_none_or(|byte| byte.is_ascii_whitespace() || matches!(byte, b'>' | b'/'));
        if !delimited {
            from = after_name;
            continue;
        }
        let Some(tag_end) = content.get(after_name..).and_then(|rest| rest.find('>')) else {
            break;
        };
        let body = after_name + tag_end + 1;
        let end = find_ascii_lowercase(content, "</script", body).unwrap_or(content.len());
        if body < end {
            regions.push(body..end);
        }
        from = end;
    }
    regions
}

/// Astro's leading `---` fence: the component's script.
fn frontmatter(content: &str) -> Option<Range<usize>> {
    let first_line = content.lines().next()?;
    if first_line.trim() != "---" {
        return None;
    }
    let body = first_line.len() + 1;
    let rest = content.get(body..)?;
    let mut offset = body;
    for line in rest.split_inclusive('\n') {
        if line.trim() == "---" {
            return Some(body..offset);
        }
        offset += line.len();
    }
    None
}

/// MDX module statements: each line that starts an `import` or an `export …
/// from`, through the line closing its module string.
fn import_lines(content: &str) -> Vec<Range<usize>> {
    let mut regions = Vec::new();
    let mut offset = 0;
    let mut open: Option<usize> = None;
    for line in content.split_inclusive('\n') {
        let start =
            line.starts_with("import ") || (line.starts_with("export ") && line.contains(" from "));
        if open.is_none() && start {
            open = Some(offset);
        }
        offset += line.len();
        if let Some(begin) = open
            && (line.contains(['"', '\'']) || line.trim_end().ends_with(';'))
        {
            regions.push(begin..offset);
            open = None;
        }
    }
    regions
}

/// `content` with every byte outside `regions` a space, line breaks kept: its
/// offsets and lines are the original's, and it stays valid UTF-8 because each
/// region starts and ends at an ASCII delimiter.
fn blank_outside(content: &str, regions: &[Range<usize>]) -> String {
    let mut blanked = String::with_capacity(content.len());
    let mut position = 0;
    let blank = |text: &str, out: &mut String| {
        for byte in text.bytes() {
            out.push(if matches!(byte, b'\n' | b'\r') {
                byte as char
            } else {
                ' '
            });
        }
    };
    for region in regions {
        if region.start < position {
            continue;
        }
        let (Some(before), Some(kept)) = (
            content.get(position..region.start),
            content.get(region.clone()),
        ) else {
            continue;
        };
        blank(before, &mut blanked);
        blanked.push_str(kept);
        position = region.end;
    }
    blank(content.get(position..).unwrap_or_default(), &mut blanked);
    blanked
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specifiers(content: &str, extension: &str) -> Vec<(String, u32)> {
        markup_scripts(content, &format!("a.{extension}"), extension)
            .imports
            .into_iter()
            .map(|import| (import.specifier, import.line))
            .collect()
    }

    #[test]
    fn each_kind_of_markup_names_its_script_regions() {
        let module = "./m".to_owned();
        for (extension, content, line) in [
            (
                "vue",
                "<template/>\n<script setup lang=\"ts\">\nimport { a } from './m';\n</script>\n",
                3,
            ),
            ("svelte", "<script>import a from './m';</script>\n<p/>\n", 1),
            (
                "html",
                "<p/>\n<SCRIPT type=\"module\">\nimport * as a from './m';\n</SCRIPT>\n",
                3,
            ),
            ("astro", "---\nimport a from './m';\n---\n<p/>\n", 2),
            ("mdx", "# t\n\nimport { a } from './m'\n\n<p/>\n", 3),
            ("vue", "<script>\nexport { a } from './m';\n</script>\n", 2),
        ] {
            assert_eq!(
                specifiers(content, extension),
                [(module.clone(), line)],
                "{content}"
            );
        }
    }

    /// Non-ASCII text outside a script keeps every offset valid.
    #[test]
    fn multibyte_markup_keeps_its_lines() {
        assert_eq!(
            specifiers(
                "<p>héllo — wörld</p>\n<script>import a from './m';</script>\n",
                "vue"
            ),
            [("./m".to_owned(), 2)]
        );
    }

    #[test]
    fn markup_without_a_script_holds_nothing() {
        for (extension, content) in [
            ("html", "<p>import nothing from './m';</p>\n"),
            ("vue", "<template><scripted/></template>\n"),
            ("mdx", "# Title\n\nimporting prose\n"),
            ("astro", "<p>--- import a from './m' ---</p>\n"),
        ] {
            let scripts = markup_scripts(content, "a", extension);
            assert_eq!(scripts, MarkupScripts::default(), "{content}");
        }
    }

    /// A script that imports nothing is still a script the graph does not read.
    #[test]
    fn a_script_without_imports_is_disclosed_and_not_parsed() {
        let scripts = markup_scripts("<script>run();</script>\n", "a.html", "html");
        assert!(scripts.has_script);
        assert!(scripts.imports.is_empty());
    }
}
