use super::*;
#[test]
fn preserves_precedence_and_cross_platform_paths() {
    assert_eq!(
        classify_file_type("src/vite.config.js"),
        Some(FileType::Config)
    );
    assert_eq!(classify_file_type("README.md"), Some(FileType::Doc));
    assert_eq!(classify_file_type("install.sh"), Some(FileType::Code));
    assert_eq!(classify_file_type("asset.bin"), None);
}
#[cfg(windows)]
#[test]
fn a_backslash_separates_windows_components() {
    assert_eq!(
        classify_file_type(r"C:\repo\Cargo.lock"),
        Some(FileType::Lock)
    );
}
/// On Unix `\` is a file-name byte: `f\README` is not the name `README`,
/// so only a rule on the extension (`.lock`, `.md`) classifies such a name.
#[cfg(unix)]
#[test]
fn a_backslash_in_a_unix_file_name_is_part_of_the_name() {
    assert_eq!(classify_file_type(r"f\Cargo.lock"), Some(FileType::Lock));
    assert_eq!(classify_file_type(r"f\README.md"), Some(FileType::Doc));
    assert_eq!(classify_file_type(r"f\README"), None);
}
#[test]
fn covers_variadic_config_and_docs() {
    for name in [
        ".eslintrc.cjs",
        ".env.production",
        "tsconfig.build.json",
        "a.csproj",
        "main.tfvars",
        "service.CONF",
    ] {
        assert!(is_config_file(name), "{name}");
    }
    for name in ["LICENSE-MIT", "CHANGELOG.1", "CONTRIBUTING", "guide.rst"] {
        assert_eq!(classify_file_type(name), Some(FileType::Doc), "{name}");
    }
}
