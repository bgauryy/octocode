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
