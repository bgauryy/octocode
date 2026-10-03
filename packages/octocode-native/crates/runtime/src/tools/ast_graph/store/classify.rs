//! File roles inferred at ingest: tests, generated code, bundled/minified
//! output, vendored third-party code, type declarations, and configuration.
//!
//! Detectors use roles as false-positive controls (a bundle is not a god
//! file, a generated client is not dead code). Classification reads at most
//! [`HEAD_BYTES`] per file and never fails ingest: an unreadable file simply
//! gets no content-based role.
use std::io::Read;
use std::path::Path;

/// Found but not parsed (over the per-file size bound): a file node with
/// no symbols. File nodes only; bit 0 means `exported` on symbols.
pub(crate) const ROLE_UNPARSED: u8 = 1;
pub(crate) const ROLE_TEST: u8 = 1 << 1;
pub(crate) const ROLE_GENERATED: u8 = 1 << 2;
pub(crate) const ROLE_BUNDLED: u8 = 1 << 3;
pub(crate) const ROLE_VENDORED: u8 = 1 << 4;
pub(crate) const ROLE_DECLARATION: u8 = 1 << 5;
pub(crate) const ROLE_CONFIG: u8 = 1 << 6;
pub(crate) const ROLE_ENTRY: u8 = 1 << 7;

pub(crate) const ROLES: &[(u8, &str)] = &[
    (ROLE_UNPARSED, "unparsed"),
    (ROLE_TEST, "test"),
    (ROLE_GENERATED, "generated"),
    (ROLE_BUNDLED, "bundled"),
    (ROLE_VENDORED, "vendored"),
    (ROLE_DECLARATION, "declaration"),
    (ROLE_CONFIG, "config"),
    (ROLE_ENTRY, "entry"),
];

/// Roles that mark code nobody maintains by hand; detectors skip them.
pub(crate) const ROLE_NOT_AUTHORED: u8 = ROLE_GENERATED | ROLE_BUNDLED | ROLE_VENDORED;

const HEAD_BYTES: usize = 8 * 1024;
/// A line this long in source is emitted by a minifier/bundler, not a person.
const MINIFIED_LINE: usize = 1_000;
const MINIFIED_AVERAGE_LINE: usize = 300;

pub(crate) fn role_names(flags: u8) -> Vec<&'static str> {
    ROLES
        .iter()
        .filter(|(bit, _)| flags & bit != 0)
        .map(|(_, name)| *name)
        .collect()
}

fn path_roles(path: &str) -> u8 {
    let lower = path.to_ascii_lowercase();
    let segments = lower.split('/').collect::<Vec<_>>();
    let name = segments.last().copied().unwrap_or_default();
    let dirs = &segments[..segments.len().saturating_sub(1)];
    let mut roles = 0;

    // Beyond tests proper, examples, benchmarks, stories, and test setup
    // files are not production code either.
    let support_dir = dirs.iter().any(|dir| {
        matches!(
            *dir,
            "testing" | "examples" | "example" | "benches" | "benchmarks"
        )
    });
    let support_name = [".bench.", ".stories."]
        .iter()
        .any(|marker| name.contains(marker))
        || name.starts_with("setuptests.")
        || name.starts_with("jest.setup")
        || name.starts_with("vitest.setup")
        || name.starts_with("test-setup.")
        || name.starts_with("testsetup.");
    if crate::content::is_test_path(path) || support_dir || support_name {
        roles |= ROLE_TEST;
    }

    if dirs.iter().any(|dir| {
        matches!(
            *dir,
            "vendor"
                | "deps"
                | "vendored"
                | "third_party"
                | "third-party"
                | "thirdparty"
                | "bower_components"
                | "jspm_packages"
        )
    }) {
        roles |= ROLE_VENDORED;
    }

    let generated_dir = dirs
        .iter()
        .any(|dir| matches!(*dir, "generated" | "__generated__" | "gen" | "autogen"));
    let generated_name = [
        ".generated.",
        ".gen.",
        "_generated.",
        ".pb.",
        "_pb2.",
        "_pb2_grpc.",
    ]
    .iter()
    .any(|marker| name.contains(marker))
        || name.ends_with(".g.dart")
        || name.ends_with(".designer.cs");
    if generated_dir || generated_name {
        roles |= ROLE_GENERATED;
    }

    if [".min.", ".bundle.", ".umd.", ".chunk."]
        .iter()
        .any(|marker| name.contains(marker))
        || name.starts_with("chunk-")
        || dirs.iter().any(|dir| {
            matches!(
                *dir,
                "dist" | "build" | "out" | ".next" | ".output" | ".nuxt"
            )
        }) && (name.ends_with(".js") || name.ends_with(".mjs") || name.ends_with(".cjs"))
    {
        roles |= ROLE_BUNDLED;
    }

    if name.ends_with(".d.ts")
        || name.ends_with(".d.mts")
        || name.ends_with(".d.cts")
        || name.ends_with(".pyi")
    {
        roles |= ROLE_DECLARATION;
    }

    let stem = name.split('.').next().unwrap_or_default();
    if name.contains(".config.")
        || name.starts_with(".eslintrc")
        || matches!(
            stem,
            "setup"
                | "conftest"
                | "build"
                | "gulpfile"
                | "gruntfile"
                | "jest"
                | "vitest"
                | "webpack"
                | "rollup"
                | "vite"
                | "babel"
                | "tsup"
                | "esbuild"
        ) && !dirs.contains(&"src")
    {
        roles |= ROLE_CONFIG;
    }
    roles
}

fn content_roles(head: &str) -> u8 {
    let mut roles = 0;
    let top = head.get(..head.len().min(2048)).unwrap_or(head);
    let lower = top.to_ascii_lowercase();
    if lower.contains("@generated")
        || lower.contains("code generated")
        || lower.contains("do not edit")
        || lower.contains("auto-generated")
        || lower.contains("autogenerated")
        || lower.contains("this file was generated")
        || lower.contains("this file is generated")
    {
        roles |= ROLE_GENERATED;
    }
    let lines = head.lines().collect::<Vec<_>>();
    let longest = lines.iter().map(|line| line.len()).max().unwrap_or(0);
    let average = if lines.is_empty() {
        0
    } else {
        head.len() / lines.len()
    };
    if head.len() >= 2048 && (longest >= MINIFIED_LINE || average >= MINIFIED_AVERAGE_LINE) {
        roles |= ROLE_BUNDLED;
    }
    if head.contains("//# sourceMappingURL=")
        || head.contains("__webpack_require__")
        || head.contains("__webpack_modules__")
        || head.contains("System.register(")
        || head.contains("/******/ (() => {")
        || head.contains("parcelRequire")
    {
        roles |= ROLE_BUNDLED;
    }
    roles
}

/// Roles for `relative` under `root`, from its path and its first 8 KiB.
pub(crate) fn file_roles(root: &Path, relative: &str) -> u8 {
    let mut roles = path_roles(relative);
    let mut buffer = Vec::with_capacity(HEAD_BYTES);
    if let Ok(file) = std::fs::File::open(root.join(relative)) {
        let _ = file.take(HEAD_BYTES as u64).read_to_end(&mut buffer);
        roles |= content_roles(&String::from_utf8_lossy(&buffer));
    }
    roles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_classify_tests_vendored_generated_and_declarations() {
        assert_eq!(path_roles("src/a.test.ts"), ROLE_TEST);
        assert_eq!(path_roles("pkg/x_test.go"), ROLE_TEST);
        assert_eq!(path_roles("tests/test_api.py"), ROLE_TEST);
        assert_eq!(path_roles("vendor/lib/a.go"), ROLE_VENDORED);
        assert_eq!(path_roles("api/v1/service.pb.go"), ROLE_GENERATED);
        assert_eq!(path_roles("types/index.d.ts"), ROLE_DECLARATION);
        assert_eq!(path_roles("static/app.min.js"), ROLE_BUNDLED);
        assert_eq!(path_roles("dist/index.js"), ROLE_BUNDLED);
        assert_eq!(path_roles("vite.config.ts"), ROLE_CONFIG);
        assert_eq!(path_roles("src/server/router.ts"), 0);
    }

    #[test]
    fn content_detects_generated_headers_and_minified_bundles() {
        assert_eq!(
            content_roles("// Code generated by protoc-gen-go. DO NOT EDIT.\npackage x\n"),
            ROLE_GENERATED
        );
        let minified = format!("!function(){{{}}}();", "var a=1;".repeat(400));
        assert_eq!(content_roles(&minified), ROLE_BUNDLED);
        assert_eq!(
            content_roles("export const a = 1;\n//# sourceMappingURL=a.js.map\n"),
            ROLE_BUNDLED
        );
        let authored = "export function a() {\n  return 1;\n}\n".repeat(100);
        assert_eq!(content_roles(&authored), 0);
    }
}
