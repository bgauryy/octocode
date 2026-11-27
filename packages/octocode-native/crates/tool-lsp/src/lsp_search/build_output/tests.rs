//! Build-output mapping over a two-package workspace: a core package that
//! emits `dist/**.d.ts` from `src`, and a host that imports it by name.
use super::*;
use octocode_runtime_base::policy::path::{PathPolicy, PathPolicyConfig};

fn policy(root: &Path) -> PathPolicy {
    PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.to_path_buf()),
        ..PathPolicyConfig::default()
    })
    .expect("path policy")
}

fn write(root: &Path, file: &str, text: &str) -> PathBuf {
    let path = root.join(file);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(&path, text).expect("write");
    std::fs::canonicalize(&path).expect("canonical")
}

/// The octocode-mcp-host layout: comments and a trailing comma in the
/// config, `outDir` and `rootDir` set, no `declarationMap`.
fn workspace() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(root, "packages/core/package.json", r#"{"name":"@x/core"}"#);
    write(
        root,
        "packages/core/tsconfig.json",
        "{\n  // bundler resolution\n  \"compilerOptions\": { \"outDir\": \"./dist\", \"rootDir\": \"src\", /* emit */ \"declaration\": true, },\n  \"include\": [\"src/**/*\"],\n}\n",
    );
    let source = write(
        root,
        "packages/core/src/toolContract/instructions.ts",
        "export function mcpInstructions(enabled: string[]): string {\n  return enabled.join(',');\n}\n",
    );
    let emitted = write(
        root,
        "packages/core/dist/toolContract/instructions.d.ts",
        "export declare function mcpInstructions(enabled: string[]): string;\n",
    );
    (dir, source, emitted)
}

#[test]
fn a_declaration_in_the_out_dir_maps_back_to_its_source() {
    let (dir, source, emitted) = workspace();
    let paths = policy(dir.path());
    let output = BuildOutput::containing(&emitted, &paths).expect("output");
    assert!(output.holds(&emitted));
    assert_eq!(output.source_of(&emitted), Some(source.clone()));
    // The source's own package names the same output.
    assert_eq!(BuildOutput::of_source(&source, &paths), Some(output));
}

#[test]
fn a_source_is_never_build_output_and_a_stale_declaration_maps_nowhere() {
    let (dir, source, _) = workspace();
    let paths = policy(dir.path());
    assert!(BuildOutput::containing(&source, &paths).is_none());
    let stale = write(
        dir.path(),
        "packages/core/dist/removed.d.ts",
        "export declare const gone: number;\n",
    );
    let output = BuildOutput::containing(&stale, &paths).expect("output");
    assert!(output.holds(&stale));
    assert_eq!(output.source_of(&stale), None);
}

#[test]
fn without_root_dir_the_include_root_is_the_mirrored_tree() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(root, "pkg/package.json", "{}");
    write(
        root,
        "pkg/tsconfig.json",
        r#"{"compilerOptions":{"outDir":"lib"},"include":["src"]}"#,
    );
    let source = write(root, "pkg/src/a.mts", "export const a = 1;\n");
    let emitted = write(root, "pkg/lib/a.d.mts", "export declare const a = 1;\n");
    let paths = policy(root);
    let output = BuildOutput::containing(&emitted, &paths).expect("output");
    assert_eq!(output.source_of(&emitted), Some(source));
}

#[test]
fn an_extended_out_dir_resolves_beside_the_config_that_names_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(
        root,
        "tsconfig.base.json",
        r#"{"compilerOptions":{"outDir":"build/out"}}"#,
    );
    write(
        root,
        "tsconfig.json",
        r#"{"extends":"./tsconfig.base","compilerOptions":{"rootDir":"src"}}"#,
    );
    let source = write(root, "src/b.ts", "export const b = 1;\n");
    let emitted = write(root, "build/out/b.d.ts", "export declare const b = 1;\n");
    let paths = policy(root);
    let output = BuildOutput::containing(&emitted, &paths).expect("output");
    assert_eq!(output.source_of(&emitted), Some(source));
}

#[test]
fn a_config_without_out_dir_emits_nothing_to_map() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(root, "package.json", "{}");
    write(
        root,
        "tsconfig.json",
        r#"{"compilerOptions":{"strict":true}}"#,
    );
    let source = write(root, "src/c.ts", "export const c = 1;\n");
    let declaration = write(root, "src/c.d.ts", "export declare const c = 1;\n");
    let paths = policy(root);
    assert!(BuildOutput::of_source(&source, &paths).is_none());
    assert!(BuildOutput::containing(&declaration, &paths).is_none());
}

#[test]
fn jsonc_comments_and_trailing_commas_are_dropped_but_strings_kept() {
    let text = "{ \"a\": \"//not a comment\", /* x */ \"b\": [1, 2,], // tail\n }";
    let value: Value = serde_json::from_str(&strip_jsonc(text)).expect("json");
    assert_eq!(value["a"], "//not a comment");
    assert_eq!(value["b"], serde_json::json!([1, 2]));
}

/// A package that emits its declarations from a second config
/// (`tsc -p tsconfig.types.json`) beside a no-emit `tsconfig.json`: the
/// emitting config names the output, whatever its name, so its
/// declarations map back to the sources (the agents-communication layout,
/// `rootDir` above the package).
#[test]
fn a_named_emit_config_beside_tsconfig_names_the_output() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(root, "packages/comms/package.json", r#"{"name":"comms"}"#);
    write(
        root,
        "packages/comms/tsconfig.json",
        r#"{"compilerOptions":{"noEmit":true},"include":["src/**/*.ts","tests/**/*.ts"]}"#,
    );
    write(
        root,
        "packages/comms/tsconfig.types.json",
        r#"{"extends":"./tsconfig.json","compilerOptions":{"noEmit":false,"declaration":true,"outDir":"dist/types","rootDir":".."},"include":["src/entries/*.ts"]}"#,
    );
    let source = write(
        root,
        "packages/comms/src/runtime/validation.ts",
        "export function asObject(value: unknown): object {\n  return value as object;\n}\n",
    );
    let emitted = write(
        root,
        "packages/comms/dist/types/comms/src/runtime/validation.d.ts",
        "export declare function asObject(value: unknown): object;\n",
    );
    let paths = policy(root);
    let output = BuildOutput::containing(&emitted, &paths).expect("output");
    assert_eq!(output.source_of(&emitted), Some(source.clone()));
    assert_eq!(BuildOutput::of_source(&source, &paths), Some(output));
}
