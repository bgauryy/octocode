use super::*;
#[test]
fn headings_skip_fences_and_preserve_hierarchy() {
    let text = "# Root\n```\n## hidden\n```\n  ## Child ###\n";
    assert_eq!(
        markdown_heading_outline(text, "README.md").as_deref(),
        Some("1\t# Root\n5\t  ## Child")
    );
    assert_eq!(markdown_heading_outline(text, "README.txt"), None);
}

#[test]
fn indented_code_never_becomes_a_heading() {
    let text = "# Root\n    # hidden\n\t# hidden\n \t# hidden\n   # Visible\n";
    assert_eq!(
        markdown_heading_outline(text, "README.md").as_deref(),
        Some("1\t# Root\n5\t# Visible")
    );
    assert_eq!(
        markdown_heading_outline("    # code\n\t## code\n", "a.md"),
        None
    );
}

#[test]
fn heading_hashes_are_removed_only_from_valid_closing_sequences() {
    let text = "# C#\n# C###\n# Escaped \\#\n# Escaped \\###\n# Closed ###\n# \\# ###\n# ###\n# Escaped \\### ###\n";
    assert_eq!(
        markdown_heading_outline(text, "README.md").as_deref(),
        Some(
            "1\t# C#\n2\t# C###\n3\t# Escaped \\#\n4\t# Escaped \\###\n5\t# Closed\n6\t# \\#\n7\t# (untitled heading)\n8\t# Escaped \\###"
        )
    );
}

#[test]
fn unicode_whitespace_is_heading_content_not_closing_syntax() {
    for space in ['\u{a0}', '\u{2003}'] {
        for newline in ["\n", "\r\n"] {
            let text = format!(
                "# {space}Title{space}{newline}# Title ###{space}{newline}# {space}{newline}# \tTitle ### \t{newline}"
            );
            assert_eq!(
                markdown_heading_outline(&text, "README.md"),
                Some(format!(
                    "1\t# {space}Title{space}\n2\t# Title ###{space}\n3\t# {space}\n4\t# Title"
                ))
            );
        }
    }
}

#[test]
fn fences_require_matching_long_enough_unindented_closing_runs() {
    for (marker, other) in [('`', '~'), ('~', '`')] {
        let four = marker.to_string().repeat(4);
        let text = format!(
            "# Root\n{four}text\n{}\n## Short hidden\n{}\n## Mismatch hidden\n{four}not-a-close\n## Tail hidden\n    {four}\n## Indent hidden\n{} \t\n## Visible\n",
            marker.to_string().repeat(3),
            other.to_string().repeat(4),
            marker.to_string().repeat(5),
        );
        assert_eq!(
            markdown_heading_outline(&text, "README.md"),
            Some(format!("1\t# Root\n{}\t  ## Visible", text.lines().count()))
        );
    }
}

#[test]
fn backticks_in_info_prevent_only_backtick_fence_openers() {
    assert_eq!(
        markdown_heading_outline("```bad`info\n# Visible\n", "a.md").as_deref(),
        Some("2\t# Visible")
    );
    assert_eq!(
        markdown_heading_outline("~~~bad`info\n# Hidden\n~~~\n# Visible\n", "a.md").as_deref(),
        Some("4\t# Visible")
    );
}

#[test]
fn a_fence_closer_accepts_only_spaces_and_tabs_after_its_run() {
    let text = "~~~\n# Hidden\n~~~\u{a0}\n# Still hidden\n~~~ \t\n# Visible\n";
    assert_eq!(
        markdown_heading_outline(text, "a.md").as_deref(),
        Some("6\t# Visible")
    );
}
