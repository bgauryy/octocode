//! A Markdown file's sections: the one outline a block read widens to
//! ([`super::block`]) and a search hit names its section by, so the range
//! a hit states is the range `block:true` reads.

/// One section of a Markdown file, 1-based lines: its heading through the
/// line before the next heading of the same or a higher level, else the
/// file's last line, so a section holds its subsections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// The heading's text.
    pub name: String,
    /// The heading's level, 1 to 6.
    pub level: usize,
    /// The heading's line: the section's first.
    pub line: usize,
    /// The section's last line.
    pub end: usize,
}

/// The sections of the Markdown file at `path`, in line order, from the
/// headings `view:"symbols"` lists
/// ([`octocode_runtime_base::content::markdown_headings`]); `None` for a
/// path that is no Markdown file.
pub fn sections(content: &str, path: &str) -> Option<Vec<Section>> {
    let headings = octocode_runtime_base::content::markdown_headings(content, path)?;
    let last = content.lines().count();
    let sections = headings
        .iter()
        .enumerate()
        .map(|(at, heading)| Section {
            name: heading.text.clone(),
            level: heading.level,
            line: heading.line,
            end: headings[at + 1..]
                .iter()
                .find(|next| next.level <= heading.level)
                .map_or(last, |next| next.line - 1),
        })
        .collect();
    Some(sections)
}

/// Whether the file at `path` has sections ([`sections`] reads it as
/// Markdown), whatever it holds.
pub fn sectioned(path: &str) -> bool {
    octocode_runtime_base::content::markdown_headings("", path).is_some()
}

/// The innermost of `sections` holding `line`: the last one that starts at
/// or before it and ends at or after it.
pub fn innermost(sections: &[Section], line: usize) -> Option<&Section> {
    sections
        .iter()
        .rev()
        .find(|section| section.line <= line && line <= section.end)
}

#[cfg(test)]
mod tests {
    use super::{innermost, sectioned, sections};

    const GUIDE: &str = "# Guide\n\nIntro.\n\n## Setup\n\nRun it.\n\n### Keys\n\nA key.\n\n```sh\n# not a heading\n```\n\n## Usage\n\nCall it.\n";

    /// Every line of a Markdown file sits in the section of the nearest
    /// heading above it that no heading of the same or a higher level
    /// closed first; each section ends where the next such heading starts.
    #[test]
    fn a_line_sits_in_the_section_of_the_nearest_open_heading() {
        let listed = sections(GUIDE, "docs/GUIDE.md").expect("markdown");
        let headings = octocode_runtime_base::content::markdown_headings(GUIDE, "docs/GUIDE.md")
            .expect("headings");
        assert_eq!(listed.len(), headings.len());
        let last = GUIDE.lines().count();
        for line in 1..=last {
            // The nearest heading at or above the line, then outwards while
            // a heading of its level or higher sits between it and the line.
            let expected = headings
                .iter()
                .rev()
                .filter(|heading| heading.line <= line)
                .find(|heading| {
                    !headings.iter().any(|next| {
                        next.line > heading.line && next.line <= line && next.level <= heading.level
                    })
                })
                .map(|heading| heading.line);
            let found = innermost(&listed, line).map(|section| section.line);
            assert_eq!(found, expected, "line {line}");
        }
        for section in &listed {
            assert!(section.line <= section.end && section.end <= last);
        }
        // A fenced `#` line is no heading: it sits in the section around it.
        let fenced = GUIDE
            .lines()
            .position(|line| line == "# not a heading")
            .expect("fenced line")
            + 1;
        assert_eq!(
            innermost(&listed, fenced).map(|section| section.name.as_str()),
            Some("Keys")
        );
    }

    #[test]
    fn only_a_markdown_path_has_sections() {
        for (path, expected) in [
            ("README.md", true),
            ("docs/a.mdx", true),
            ("src/lib.rs", false),
        ] {
            assert_eq!(sectioned(path), expected, "{path}");
            assert_eq!(sections(GUIDE, path).is_some(), expected, "{path}");
        }
    }
}
