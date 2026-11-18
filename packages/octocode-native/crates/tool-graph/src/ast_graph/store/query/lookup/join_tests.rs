/// A snapshot key named from the workspace root: the scope joined in,
/// a config file above the scope folded in, an outside scope kept whole.
#[test]
fn a_key_joins_its_scope_lexically() {
    for (root, key, shown) in [
        (
            "packages/cli",
            "src/json-schema.ts",
            "packages/cli/src/json-schema.ts",
        ),
        ("app", "../package.json", "package.json"),
        ("a/b", "../../../x.json", "../x.json"),
        ("/outside/scope", "src/a.ts", "/outside/scope/src/a.ts"),
        ("app", "/abs/a.ts", "/abs/a.ts"),
    ] {
        assert_eq!(super::join(root, key), shown, "{root} + {key}");
    }
}
