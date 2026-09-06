//! Exhaustive routing checks complement language-specific syntax fixtures.
//! These cover configured strategies and comment groups, not every construct
//! in each language. Adding a configured extension automatically adds cases.
use super::apply::{apply_content_view_minification_inner, apply_minification_inner};
use super::comment_remover::rules_for;
use super::config::{indentation_sensitive_names, minify_config, FileTypeConfig};
use super::minifier::get_file_config;

const MARKER: &str = "octocodeKeepMarker";
const LITERAL: &str = "https://example.com/a//literal";
const COMMENT: &str = "octocode removable comment";

#[test]
fn research_views_preserve_multiline_literal_payloads() {
    let payload = "alpha  \r\n\r\n\r\nbeta\t\r\n";
    for (extension, source) in [
        ("py", format!("value = \"\"\"{payload}\"\"\"\n")),
        ("rs", format!("pub const VALUE: &str = r#\"{payload}\"#;\n")),
        ("go", format!("var value = `{payload}`\n")),
    ] {
        for output in [
            apply_content_view_minification_inner(&source, &format!("literal.{extension}")),
            apply_minification_inner(&source, &format!("literal.{extension}")),
        ] {
            assert!(
                output.contains(payload),
                "{extension}: changed literal: {output:?}"
            );
        }
    }
}

#[test]
fn research_web_views_preserve_raw_regions_and_script_literals() {
    let payload = "alpha  \n\n\nbeta\t\n";
    for extension in ["html", "htm", "vue", "svelte"] {
        let source = format!("<!-- removable -->\n<script>\nconst marker = \"<!-- retained literal -->\";\nconsole.log(marker);\n</script>\n<pre>{payload}</pre>\n<textarea>{payload}</textarea>\n<script type=\"application/json\">{{\"value\":\"<!-- json literal -->\"}}</script>\n");
        let output =
            apply_content_view_minification_inner(&source, &format!("literal.{extension}"));
        assert!(
            output.contains("<!-- retained literal -->"),
            "{extension}: {output}"
        );
        assert!(
            output.contains("<!-- json literal -->"),
            "{extension}: {output}"
        );
        assert!(
            output.contains(&format!("<pre>{payload}</pre>")),
            "{extension}: {output}"
        );
        assert!(
            output.contains(&format!("<textarea>{payload}</textarea>")),
            "{extension}: {output}"
        );
        assert!(!output.contains("<!-- removable -->"));
    }
}

fn fixture(extension: &str, config: &FileTypeConfig, group: Option<&str>) -> String {
    let comment = group.map_or_else(String::new, |group| {
        if group == "python-docstring" {
            return format!("\"\"\"{COMMENT}\"\"\"\n");
        }
        let rules = rules_for(group).unwrap_or_else(|| panic!("unknown comment group: {group}"));
        if let Some(rule) = rules.line.first() {
            format!("{} {COMMENT}\n", rule.token)
        } else if let Some(rule) = rules.block.first() {
            format!("{} {COMMENT} {}\n", rule.start, rule.end)
        } else {
            panic!("no comment fixture for group: {group}")
        }
    });
    let body = if crate::text::file_extension::is_js_ts_extension(extension) {
        format!("export function {MARKER}() {{\n  return \"{LITERAL}\";\n}}\n")
    } else if matches!(extension, "css" | "scss" | "less" | "sass") {
        format!(".{MARKER} {{\n  content: \"{LITERAL}\";\n}}\n")
    } else if matches!(extension, "html" | "htm" | "vue" | "svelte" | "xml" | "svg") {
        format!("<div id=\"{MARKER}\">{LITERAL}</div>\n")
    } else if config.strategy == "json" {
        format!("{{\n  \"{MARKER}\": \"{LITERAL}\"\n}}\n")
    } else if config.strategy == "markdown" {
        format!("# {MARKER}\n\n{LITERAL}\n")
    } else {
        format!("{MARKER} = \"{LITERAL}\"\n")
    };
    format!("\n{comment}\n{body}\n\n")
}

#[test]
fn every_structural_extension_has_an_explicit_minification_route() {
    let extensions = crate::signatures::languages::supported_extensions();
    assert_eq!(extensions.len(), 45);
    for extension in extensions {
        assert!(
            minify_config().contains_key(extension),
            "{extension}: implicit general fallback"
        );
    }
}

#[test]
fn every_configured_extension_preserves_evidence_and_removes_its_comments() {
    let config = minify_config();
    assert_eq!(
        config.len(),
        151,
        "review new entries and update the published inventory"
    );
    for (&extension, cfg) in config {
        let groups: Vec<_> = cfg.comments.map_or_else(
            || vec![None],
            |groups| groups.iter().copied().map(Some).collect(),
        );
        for group in groups {
            let source = fixture(extension, cfg, group);
            let path = format!("fixture.{extension}");
            for (mode, output) in [
                (
                    "standard",
                    apply_content_view_minification_inner(&source, &path),
                ),
                ("full", apply_minification_inner(&source, &path)),
            ] {
                assert!(
                    output.contains(MARKER),
                    "{extension}/{group:?}/{mode}: marker lost: {output}"
                );
                assert!(
                    output.contains(LITERAL),
                    "{extension}/{group:?}/{mode}: string literal changed: {output}"
                );
                assert!(
                    !output.contains(COMMENT),
                    "{extension}/{group:?}/{mode}: comment retained: {output}"
                );
                assert!(
                    output.len() <= source.len(),
                    "{extension}/{mode}: output grew"
                );
                let repeat = if mode == "standard" {
                    apply_content_view_minification_inner(&source, &path)
                } else {
                    apply_minification_inner(&source, &path)
                };
                assert_eq!(
                    output, repeat,
                    "{extension}/{mode}: nondeterministic output"
                );
            }
        }
    }
}

#[test]
fn every_basename_override_preserves_recipe_indentation_and_literals() {
    assert_eq!(indentation_sensitive_names().len(), 15);
    let source =
        "all:\n\t# octocode removable comment\n\techo \"https://example.com/a//literal\"\n\n";
    for name in indentation_sensitive_names() {
        for path in [
            name.to_string(),
            name.to_uppercase(),
            format!("nested/{name}"),
        ] {
            assert_eq!(get_file_config(&path).unwrap().strategy, "conservative");
            for output in [
                apply_content_view_minification_inner(source, &path),
                apply_minification_inner(source, &path),
            ] {
                assert!(
                    output.contains("\techo"),
                    "{path}: recipe tab lost: {output}"
                );
                assert!(
                    output.contains(LITERAL),
                    "{path}: literal changed: {output}"
                );
                assert!(
                    !output.contains(COMMENT),
                    "{path}: comment retained: {output}"
                );
            }
        }
    }
}

#[test]
fn embedded_script_content_views_preserve_types_and_binding_references() {
    for extension in ["html", "vue", "svelte"] {
        let source = "<script lang=\"ts\">\ninterface PrivateShape { name: string; }\nexport function target(value: PrivateShape) {\n  const localName = value.name;\n  return localName;\n}\n</script>\n<div>content</div>\n";
        let output = apply_content_view_minification_inner(source, &format!("fixture.{extension}"));
        assert!(
            output.contains("interface PrivateShape"),
            "{extension}: type declaration lost: {output}"
        );
        assert_eq!(
            output.matches("localName").count(),
            2,
            "{extension}: binding reference lost: {output}"
        );
    }
}
