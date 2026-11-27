use super::*;
#[test]
fn declaration_extensions_name_declaration_files() {
    for extension in DECLARATION_EXTENSIONS {
        let path = format!("types/api{extension}");
        assert!(is_declaration(&path));
    }
    assert!(!is_declaration("src/api.ts"));
}
#[test]
fn bare_specifiers_name_files_by_a_whole_trailing_path() {
    let modules = ModulesByName::new([
        "src/vs/base/common/x.ts",
        "src/dir/index.ts",
        "lib/types.d.ts",
        "src/max.ts",
    ]);
    let named = |spec| modules.named_by(spec).collect::<Vec<_>>();
    assert_eq!(named("vs/base/common/x"), ["src/vs/base/common/x.ts"]);
    assert_eq!(named("vs/base/common/x.js"), ["src/vs/base/common/x.ts"]);
    assert_eq!(named("src/dir"), ["src/dir/index.ts"]);
    assert_eq!(named("src/dir/index"), ["src/dir/index.ts"]);
    assert_eq!(named("lib/types"), ["lib/types.d.ts"]);
    // A partial segment, a single segment or a relative path names none.
    assert!(named("ase/common/x").is_empty());
    assert!(named("rc/max").is_empty());
    assert!(named("x").is_empty());
    assert!(named("./common/x").is_empty());
    assert!(named("lodash/fp").is_empty());
}
#[test]
fn build_roots_resolve_only_matching_scanned_sources() {
    let known = BTreeSet::from(["pkg/src/a.ts".to_owned(), "pkg/src/index.ts".to_owned()]);
    for dir in BUILD_DIRS {
        assert_eq!(
            source_equivalent("pkg", &format!("{dir}/a.js"), &known),
            Some("pkg/src/a.ts".to_owned())
        );
        assert_eq!(
            source_equivalent("pkg", &format!("{dir}/absent.js"), &known),
            None
        );
    }
    assert_eq!(
        source_equivalent("pkg", "dist/esm/a.d.ts", &known),
        Some("pkg/src/a.ts".to_owned())
    );
}
