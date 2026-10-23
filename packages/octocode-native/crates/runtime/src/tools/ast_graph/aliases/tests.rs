use super::*;
use crate::tools::ast_graph::test_support::{Fixture, known};

#[test]
fn indexing_keeps_python_and_include_roots_after_the_sixty_fourth() {
    let root = Fixture::new();
    let mut files = BTreeSet::new();
    for i in 0..70 {
        let dir = format!("project{i:02}");
        root.write(&format!("{dir}/pyproject.toml"), "[project]\nname='x'\n");
        root.write(&format!("{dir}/main.py"), "x = 1\n");
        files.insert(format!("{dir}/main.py"));
    }
    let commands = (0..70)
        .map(|i| serde_json::json!({"directory":root.0.as_path(),"arguments":["cc",format!("-Iinc{i:02}")]}))
        .collect::<Vec<_>>();
    root.write(
        "compile_commands.json",
        serde_json::to_string(&commands).unwrap(),
    );
    files.insert("main.c".into());
    let context = load(root.0.as_path(), &files);
    assert!(context.python_roots.contains(&"project69".to_owned()));
    assert!(context.system_include_dirs.contains(&"inc69".to_owned()));
}

fn load(root: &Path, files: &BTreeSet<String>) -> ResolveContext {
    let policy = crate::tools::test_support::workspace_policy(root);
    ResolveContext::load(root, files, &policy, &ContentSecurity::new())
}

#[test]
fn config_reads_stay_inside_the_policy_and_the_size_bound() {
    let root = Fixture::new();
    let outside = Fixture::new();
    let policy = crate::tools::test_support::workspace_policy(root.0.as_path());
    let security = ContentSecurity::new();
    root.write("package.json", r#"{"name":"inside"}"#);
    outside.write("package.json", r#"{"name":"outside"}"#);
    assert_eq!(
        read_config_text(&policy, &security, &root.0.as_path().join("package.json")).as_deref(),
        Some(r#"{"name":"inside"}"#)
    );
    assert_eq!(
        read_config_text(
            &policy,
            &security,
            &outside.0.as_path().join("package.json")
        ),
        None
    );
    root.write("big.json", " ".repeat(MAX_CONFIG_BYTES + 1));
    assert_eq!(
        read_config_text(&policy, &security, &root.0.as_path().join("big.json")),
        None
    );
}

#[test]
fn c_includes_fall_back_to_a_unique_path_suffix() {
    let dir = Fixture::new();
    let files = [
        "deps/jemalloc/include/jemalloc/internal/stats.h",
        "deps/jemalloc/src/stats.c",
        "src/server.c",
        "a/util.h",
        "b/util.h",
        "b/main.c",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<BTreeSet<_>>();
    let context = load(dir.0.as_path(), &files);
    assert_eq!(
        context
            .header_by_suffix("jemalloc/internal/stats.h", "deps/jemalloc/src/stats.c")
            .as_deref(),
        Some("deps/jemalloc/include/jemalloc/internal/stats.h")
    );
    // Ambiguous basename: the includer's own subtree wins; otherwise none.
    assert_eq!(
        context.header_by_suffix("util.h", "b/main.c").as_deref(),
        Some("b/util.h")
    );
    assert_eq!(context.header_by_suffix("util.h", "src/server.c"), None);
    assert_eq!(context.header_by_suffix("missing.h", "src/server.c"), None);
}

#[test]
fn jsonc_comments_and_trailing_commas_are_tolerated() {
    let value = parse_jsonc(
        "{\n // c\n \"a\": \"x//y/*z*/\", /* b */\n \"b\": [1, 2,],\n \"c\": {\"d\": 1,},\n}",
    )
    .unwrap();
    assert_eq!(value["a"], "x//y/*z*/");
    assert_eq!(value["b"], serde_json::json!([1, 2]));
    assert_eq!(value["c"]["d"], 1);
}

#[test]
fn tsconfig_paths_resolve_through_relative_extends_chains() {
    let dir = Fixture::new();
    let root = dir.0.as_path().canonicalize().unwrap();
    dir.write("tsconfig.base.json", "{\n  // shared\n  \"compilerOptions\": {\n    \"baseUrl\": \".\",\n    \"paths\": {\n      \"@lib/*\": [\"missing/*\", \"packages/lib/src/*\"],\n      \"@lib\": [\"packages/lib/src/index.ts\"],\n      \"@/*\": [\"packages/app/src/*\"],\n    },\n  },\n}\n");
    dir.write(
        "packages/app/tsconfig.json",
        "{ \"extends\": \"../../tsconfig.base\", \"compilerOptions\": {} }",
    );
    dir.write(
        "packages/other/tsconfig.json",
        "{ \"compilerOptions\": { \"paths\": { \"~x/*\": [\"./src/x/*\"] } } }",
    );
    let files = known(&[
        "packages/app/src/main.ts",
        "packages/app/src/util/helpers.ts",
        "packages/lib/src/index.ts",
        "packages/lib/src/deep/thing.tsx",
        "packages/other/src/main.ts",
        "packages/other/src/x/y.ts",
    ]);
    let context = load(&root, &files);
    let importer = "packages/app/src/main.ts";
    for (spec, expected) in [
        ("@lib/deep/thing", Some("packages/lib/src/deep/thing.tsx")),
        ("@lib", Some("packages/lib/src/index.ts")),
        (
            "@/util/helpers.js",
            Some("packages/app/src/util/helpers.ts"),
        ),
        ("@lib/none", None),
        ("react", None),
    ] {
        assert_eq!(
            context.resolve_bare_js(spec, importer, &files).as_deref(),
            expected,
            "{spec}"
        );
    }
    // Nearest config governs: `other` does not see the base aliases, and
    // its own `paths` are relative to its config without a baseUrl.
    assert_eq!(
        context.resolve_bare_js("@lib", "packages/other/src/main.ts", &files),
        None
    );
    assert_eq!(
        context
            .resolve_bare_js("~x/y", "packages/other/src/main.ts", &files)
            .as_deref(),
        Some("packages/other/src/x/y.ts")
    );
    assert!(context.is_internal_bare_js("@lib/none", importer));
    assert!(!context.is_internal_bare_js("react", importer));
    assert!(context.is_internal_bare_js("~/anything", importer));
}

#[test]
fn base_url_resolves_non_relative_modules_and_references_are_followed() {
    let dir = Fixture::new();
    let root = dir.0.as_path().canonicalize().unwrap();
    dir.write(
        "tsconfig.json",
        "{ \"files\": [], \"references\": [{ \"path\": \"./tsconfig.app.json\" }] }",
    );
    dir.write(
        "tsconfig.app.json",
        "{ \"compilerOptions\": { \"baseUrl\": \"src\", \"paths\": { \"@/*\": [\"./*\"] } } }",
    );
    let files = known(&["src/main.ts", "src/components/Button.tsx"]);
    let context = load(&root, &files);
    assert_eq!(
        context
            .resolve_bare_js("@/components/Button", "src/main.ts", &files)
            .as_deref(),
        Some("src/components/Button.tsx")
    );
    assert_eq!(
        context
            .resolve_bare_js("components/Button", "src/main.ts", &files)
            .as_deref(),
        Some("src/components/Button.tsx")
    );
}

#[test]
fn workspace_packages_prefer_sources_over_build_output() {
    let dir = Fixture::new();
    let root = dir.0.as_path().canonicalize().unwrap();
    dir.write("packages/core/package.json", r##"{
              "name": "@acme/core",
              "main": "./dist/cjs/index.js",
              "exports": {
                ".": { "types": "./dist/index.d.ts", "import": { "default": "./dist/esm/index.mjs" }, "require": "./dist/cjs/index.js" },
                "./feature": { "import": "./dist/esm/feature/index.js" },
                "./utils/*": "./dist/utils/*.js",
                "./package.json": "./package.json"
              },
              "imports": { "#internal/*": { "source": "./src/internal/*.ts", "default": "./dist/internal/*.js" } }
            }"##);
    dir.write(
        "packages/legacy/package.json",
        r#"{ "name": "legacy", "module": "lib/index.js", "types": "lib/index.d.ts" }"#,
    );
    let files = known(&[
        "packages/core/src/index.mts",
        "packages/core/src/feature/index.ts",
        "packages/core/src/utils/strings.ts",
        "packages/core/src/internal/secret.ts",
        "packages/core/src/deep/file.ts",
        "packages/legacy/src/index.ts",
        "packages/app/main.ts",
    ]);
    let context = load(&root, &files);
    let importer = "packages/app/main.ts";
    for (spec, expected) in [
        ("@acme/core", Some("packages/core/src/index.mts")),
        (
            "@acme/core/feature",
            Some("packages/core/src/feature/index.ts"),
        ),
        (
            "@acme/core/utils/strings",
            Some("packages/core/src/utils/strings.ts"),
        ),
        (
            "@acme/core/deep/file",
            Some("packages/core/src/deep/file.ts"),
        ),
        ("legacy", Some("packages/legacy/src/index.ts")),
        ("@acme/core/nope", None),
    ] {
        assert_eq!(
            context.resolve_bare_js(spec, importer, &files).as_deref(),
            expected,
            "{spec}"
        );
    }
    assert_eq!(
        context
            .resolve_bare_js("#internal/secret", "packages/core/src/index.mts", &files)
            .as_deref(),
        Some("packages/core/src/internal/secret.ts")
    );
    assert!(context.is_internal_bare_js("@acme/core/nope", importer));
    assert!(context.is_internal_bare_js("#internal/x", importer));
}

#[test]
fn python_roots_include_src_layout_project_roots() {
    let dir = Fixture::new();
    let root = dir.0.as_path().canonicalize().unwrap();
    dir.write("libs/tool/pyproject.toml", "[project]\nname='tool'\n");
    dir.write("libs/tool/src/tool/__init__.py", "");
    let files = known(&["libs/tool/src/tool/__init__.py", "app/main.py"]);
    let context = load(&root, &files);
    assert_eq!(
        context.python_roots_for("libs/tool/src/tool/__init__.py"),
        vec!["libs/tool/src", "libs/tool", "."]
    );
    assert_eq!(
        context.python_roots_for("app/main.py"),
        vec![".", "libs/tool", "libs/tool/src"]
    );
}

#[test]
fn subtree_scans_use_the_package_tsconfig_above_the_root() {
    let dir = Fixture::new();
    let workspace = dir.0.as_path().canonicalize().unwrap();
    dir.write(
        "app/tsconfig.json",
        "{ \"compilerOptions\": { \"paths\": { \"@ui/*\": [\"./src/ui/*\"] } } }",
    );
    dir.write("app/package.json", "{}");
    let root = workspace.join("app/src");
    let files = known(&["main.ts", "ui/button.ts"]);
    let policy = crate::tools::test_support::workspace_policy(&workspace);
    let context = ResolveContext::load(&root, &files, &policy, &ContentSecurity::new());
    assert_eq!(
        context
            .resolve_bare_js("@ui/button", "main.ts", &files)
            .as_deref(),
        Some("ui/button.ts")
    );
}

#[test]
fn compile_commands_include_dirs_are_rebased_to_the_root() {
    let dir = Fixture::new();
    let root = dir.0.as_path().canonicalize().unwrap();
    let commands = serde_json::json!([
        {"directory": root.join("build"), "command": "c++ -I../third/include -I /outside -iquote ../quoted -c ../src/a.cpp", "file": "../src/a.cpp"},
        {"directory": root, "arguments": ["cc", "-I", "gen", "-c", "b.c"], "file": "b.c"}
    ]);
    dir.write("compile_commands.json", commands.to_string());
    dir.write("src/a.cpp", "");
    let files = known(&["src/a.cpp"]);
    let context = load(&root, &files);
    assert_eq!(
        context.include_dirs(false),
        ["third/include", "quoted", "gen"].map(str::to_owned)
    );
    assert_eq!(
        context.include_dirs(true),
        [".", "src", "third/include", "quoted", "gen"].map(str::to_owned)
    );
}
