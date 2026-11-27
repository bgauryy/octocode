use super::*;
use octocode_test_support::json_fixture;

#[test]
fn unanchored_globs_stay_and_anchored_ones_are_rebased_inside() {
    let scope = |globs: &[&str]| BuildScope {
        exclude: globs.iter().map(|glob| (*glob).to_owned()).collect(),
        ..BuildScope::default()
    };
    let inside = scope(&[
        "*.gen.ts",
        "/big/skip",
        "big/inc/*.h",
        "!/big/keep/",
        "/b?g/x",
        "/**/deep",
        "/small/other",
        "/big",
    ])
    .inside("big")
    .exclude;
    assert_eq!(
        inside,
        [
            "*.gen.ts", "/skip", "/inc/*.h", "!/keep/", "/x", "/**/deep", "/**",
        ]
    );
    assert_eq!(scope(&["/a/b/c"]).inside("a/b").exclude, ["/c"]);
    // `**` matches no directory as well as several: `test/fixtures` is
    // `fixtures` under `test`, and a deeper `test/fixtures` stays matched.
    assert_eq!(
        scope(&["**/test/fixtures"]).inside("test").exclude,
        ["/**/test/fixtures", "/fixtures"]
    );
    assert!(scope(&["/a/x"]).inside("a/b").exclude.is_empty());
}

#[test]
fn globs_of_a_root_are_rebased_above_it() {
    let scope = BuildScope {
        exclude: vec!["*.gen.ts".into(), "/src/skip".into(), "!lib/x/".into()],
        language_globs: BTreeMap::from([("cpp".into(), vec!["inc/*.h".into()])]),
        ..BuildScope::default()
    };
    let above = scope.above(Path::new("packages/app"));
    assert_eq!(
        above.exclude,
        [
            "*.gen.ts",
            "/packages/app/src/skip",
            "!/packages/app/lib/x/"
        ]
    );
    assert_eq!(above.language_globs["cpp"], ["/packages/app/inc/*.h"]);
}

#[test]
fn a_manifest_scope_round_trips_into_a_row() {
    let scan = json_fixture(
        r#"{
            "exclude": ["*.gen.ts"],
            "languageGlobs": {"cpp": ["inc/*.h"]},
            "walk": {
                "defaultExcludes": true,
                "includeIgnored": false,
                "includeHidden": true
            }
        }"#,
    );
    let mut row = json_fixture(r#"{"path": "."}"#);
    BuildScope::of_manifest(&scan).write_row(&mut row);
    assert_eq!(
        row,
        json_fixture(
            r#"{
                "path": ".",
                "exclude": ["*.gen.ts"],
                "parserGlobs": {"cpp": ["inc/*.h"]},
                "defaultExcludes": true,
                "includeIgnored": false,
                "includeHidden": true
            }"#
        )
    );
}
