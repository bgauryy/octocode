use super::DocumentFormat;

#[test]
fn a_format_is_read_from_the_extension_in_any_case() {
    for (path, format) in [
        ("README.MD", Some(DocumentFormat::Markdown)),
        ("a/package.json", Some(DocumentFormat::Json)),
        ("x.yml", Some(DocumentFormat::Yaml)),
        ("Cargo.toml", Some(DocumentFormat::Toml)),
        ("lib.rs", None),
        ("noext", None),
    ] {
        assert_eq!(DocumentFormat::of(path), format, "{path}");
    }
}
