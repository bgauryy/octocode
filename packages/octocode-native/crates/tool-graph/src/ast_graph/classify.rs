//! File roles inferred at ingest: tests, generated code, bundled/minified
//! output, vendored third-party code, type declarations, and configuration.
//!
//! Detectors use roles as false-positive controls (a bundle is not a god
//! file, a generated client is not dead code). Classification reads at most
//! [`HEAD_BYTES`] per file and never fails ingest: an unreadable file simply
//! gets no content-based role.
use super::js_paths::{BUNDLED_OUTPUT_DIRS, DECLARATION_EXTENSIONS, TYPE_STUB_EXTENSIONS};
use std::io::Read;
use std::path::Path;

/// A node's role and flag bits (`super::store::format::NodeRec::flags`).
pub(crate) type Roles = u16;

/// Found but not parsed (over the per-file size bound): a file node with
/// no symbols. File nodes only; bit 0 means `exported` on symbols.
pub(crate) const ROLE_UNPARSED: Roles = 1;
/// A test file by the one shared test-code rule (the `testCode` filter's):
/// where its mounts place it, else its path
/// ([`octocode_engine::text::test_paths::is_test_path`]), or a Rust
/// `#[cfg(test)]` mount.
pub(crate) const ROLE_TEST: Roles = 1 << 1;
pub(crate) const ROLE_GENERATED: Roles = 1 << 2;
pub(crate) const ROLE_BUNDLED: Roles = 1 << 3;
pub(crate) const ROLE_VENDORED: Roles = 1 << 4;
pub(crate) const ROLE_DECLARATION: Roles = 1 << 5;
pub(crate) const ROLE_CONFIG: Roles = 1 << 6;
pub(crate) const ROLE_ENTRY: Roles = 1 << 7;
/// Code that supports the project without being production or a test by
/// the shared rule: examples, benchmarks, stories, test helpers and setup
/// ([`octocode_engine::text::support_paths::is_support_path`]).
pub(crate) const ROLE_SUPPORT: Roles = 1 << 8;

/// A read of the file hides a value of it (a secret): the snapshot's
/// digests, spans and origins of the file count text that holds it, so its
/// rows state none of them ([`super::store`] query `shows_provenance`). A
/// flag of the file, not a role: no name lists it and no filter takes it.
pub(crate) const ROLE_HIDDEN_VALUE: Roles = 1 << 9;

/// Roles of authored files that are not production code: detectors judge
/// production only.
pub(crate) const ROLE_NOT_PRODUCTION: Roles = ROLE_TEST | ROLE_SUPPORT | ROLE_CONFIG;

pub(crate) const ROLES: &[(Roles, &str)] = &[
    (ROLE_UNPARSED, "unparsed"),
    (ROLE_TEST, "test"),
    (ROLE_GENERATED, "generated"),
    (ROLE_BUNDLED, "bundled"),
    (ROLE_VENDORED, "vendored"),
    (ROLE_DECLARATION, "declaration"),
    (ROLE_CONFIG, "config"),
    (ROLE_ENTRY, "entry"),
    (ROLE_SUPPORT, "support"),
];

/// Roles that mark code nobody maintains by hand; detectors skip them.
pub(crate) const ROLE_NOT_AUTHORED: Roles = ROLE_GENERATED | ROLE_BUNDLED | ROLE_VENDORED;

/// Directory names (lowercase) whose files are vendored third-party code.
pub(crate) const VENDORED_DIRS: &[&str] = &[
    "vendor",
    "deps",
    "vendored",
    "third_party",
    "third-party",
    "thirdparty",
    "bower_components",
    "jspm_packages",
];

const HEAD_BYTES: usize = 8 * 1024;
/// A line this long in source is emitted by a minifier/bundler, not a person.
const MINIFIED_LINE: usize = 1_000;
const MINIFIED_AVERAGE_LINE: usize = 300;

pub(crate) fn role_names(flags: Roles) -> Vec<&'static str> {
    ROLES
        .iter()
        .filter(|(bit, _)| flags & bit != 0)
        .map(|(_, name)| *name)
        .collect()
}

fn path_roles(path: &str, test: bool) -> Roles {
    let lower = path.to_ascii_lowercase();
    let segments = lower.split('/').collect::<Vec<_>>();
    let name = segments.last().copied().unwrap_or_default();
    let dirs = &segments[..segments.len().saturating_sub(1)];
    let mut roles = 0;

    if test {
        roles |= ROLE_TEST;
    }
    if octocode_engine::text::support_paths::is_support_path(path) {
        roles |= ROLE_SUPPORT;
    }

    if dirs.iter().any(|dir| VENDORED_DIRS.contains(dir)) {
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
        || dirs.iter().any(|dir| BUNDLED_OUTPUT_DIRS.contains(dir))
            && (name.ends_with(".js") || name.ends_with(".mjs") || name.ends_with(".cjs"))
    {
        roles |= ROLE_BUNDLED;
    }

    if DECLARATION_EXTENSIONS
        .iter()
        .chain(TYPE_STUB_EXTENSIONS)
        .any(|extension| name.ends_with(extension))
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

/// What a generator writes in the header comment of a file it emits.
const GENERATED_MARKERS: [&str; 7] = [
    "@generated",
    "code generated",
    "do not edit",
    "auto-generated",
    "autogenerated",
    "this file was generated",
    "this file is generated",
];

/// What a line comment starts with, once its indent is trimmed.
const LINE_COMMENTS: [&str; 4] = ["//", "#", "--", ";"];

/// Block comments: what opens one at the start of a line, and what closes it.
const BLOCK_COMMENTS: [(&str, &str); 2] = [("/*", "*/"), ("<!--", "-->")];

/// Whether a comment of `top` carries a generator's marker. A marker on a
/// line of code (a string a generator script writes into its output) says
/// nothing about the file that holds it.
fn generated_header(top: &str) -> bool {
    let mut open_block: Option<&str> = None;
    top.lines().any(|line| {
        let line = line.trim_start();
        let comment = match open_block {
            Some(close) => {
                if line.contains(close) {
                    open_block = None;
                }
                true
            }
            None => match BLOCK_COMMENTS
                .iter()
                .find_map(|(open, close)| line.strip_prefix(open).map(|rest| (rest, *close)))
            {
                Some((rest, close)) => {
                    if !rest.contains(close) {
                        open_block = Some(close);
                    }
                    true
                }
                None => LINE_COMMENTS.iter().any(|start| line.starts_with(start)),
            },
        };
        comment && {
            let lower = line.to_ascii_lowercase();
            GENERATED_MARKERS
                .iter()
                .any(|marker| lower.contains(marker))
        }
    })
}

fn content_roles(head: &str) -> Roles {
    let mut roles = 0;
    let top = head.get(..head.len().min(2048)).unwrap_or(head);
    if generated_header(top) {
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

/// Roles for `relative` under `root`, from its path and its first 8 KiB:
/// the scanned source's [`SourceMarks::roles`], else a read of the file
/// (one found but not parsed). `test` is the shared test-code verdict on the
/// file's place ([`octocode_tools_shared::test_code::TestCodeIndex::test_path`]: its
/// mounts, spelling only when nothing places it), not its spelling alone.
pub(crate) fn file_roles(
    root: &Path,
    relative: &str,
    marks: Option<SourceMarks>,
    test: bool,
) -> Roles {
    let content = marks.map_or_else(
        || {
            let mut buffer = Vec::with_capacity(HEAD_BYTES);
            if let Ok(file) = std::fs::File::open(root.join(relative)) {
                let _ = file.take(HEAD_BYTES as u64).read_to_end(&mut buffer);
                head_roles(&buffer)
            } else {
                0
            }
        },
        |marks| marks.roles,
    );
    path_roles(relative, test) | content
}

fn head_roles(head: &[u8]) -> Roles {
    content_roles(&String::from_utf8_lossy(
        &head[..head.len().min(HEAD_BYTES)],
    ))
}

/// What a file's scanned source says about it, read once while it is
/// parsed: its content roles and the entrypoint markers of
/// `super::store::workspace`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SourceMarks {
    /// [`file_roles`] from the first 8 KiB (generated, bundled).
    pub roles: Roles,
    /// The source starts with `#!`.
    pub shebang: bool,
    /// A Go file whose first 4 KiB say `package main` (its first clause),
    /// or a Python file with a `__main__` guard (anywhere, usually last).
    pub main_marker: bool,
}

impl SourceMarks {
    pub(crate) fn of(relative: &str, content: &str) -> Self {
        let bytes = content.as_bytes();
        let name = relative.rsplit('/').next().unwrap_or(relative);
        let main_marker = match name.rsplit_once('.').map(|(_, ext)| ext) {
            Some("go") => {
                String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).contains("package main")
            }
            Some("py") => {
                content.contains("__name__ == \"__main__\"")
                    || content.contains("__name__ == '__main__'")
            }
            _ => false,
        };
        Self {
            roles: head_roles(bytes),
            shebang: bytes.starts_with(b"#!"),
            main_marker,
        }
    }

    /// These marks of a source a read hides a value of when `hidden`
    /// ([`ROLE_HIDDEN_VALUE`]).
    pub(crate) fn hiding(mut self, hidden: bool) -> Self {
        if hidden {
            self.roles |= ROLE_HIDDEN_VALUE;
        }
        self
    }
}

#[cfg(test)]
mod tests;
