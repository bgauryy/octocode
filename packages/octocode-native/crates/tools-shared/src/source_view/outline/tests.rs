use super::*;

#[test]
fn markdown_without_atx_headings_reports_no_declarations() {
    for path in ["README.md", "guide.MARKDOWN", "page.mdx"] {
        for source in [
            "",
            "Ordinary prose.\n",
            "Title\n=====\n",
            "```\n# Code\n```\n",
        ] {
            let view = render_symbols_view(source, path);
            assert_eq!(
                view.outline,
                Err(OutlineUnavailable::NoDeclarations),
                "{path}: {source:?}"
            );
        }
    }
    assert_eq!(
        render_symbols_view("Ordinary prose.\n", "notes.txt").outline,
        Err(OutlineUnavailable::UnsupportedLanguage)
    );
}
