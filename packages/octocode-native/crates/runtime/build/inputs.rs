//! The files outside a crate's own sources that the build identity hashes:
//! the contract `@octocodeai/config` generates, and the crates the runtime
//! links. The contract file list is `octocode-runtime-base`'s, whose build
//! embeds and renders it ([`contract`]), so a file that build reads is never
//! missing from the identity.

use contract::{CONFIG_CONTRACT, CONTRACT_FILES, config_path};
use std::collections::HashSet;
use std::error::Error;
use std::path::{Component, Path, PathBuf};

#[path = "../../runtime-base/build/inputs.rs"]
mod contract;

/// Every contract input, as `(label, path)`; the label does not depend on where
/// the checkout lives.
pub(super) fn contract_inputs(manifest_dir: &Path) -> Vec<(String, PathBuf)> {
    let contract = CONTRACT_FILES.iter().map(|name| format!("contract/{name}"));
    std::iter::once(CONFIG_CONTRACT.to_string())
        .chain(contract)
        .map(|relative| {
            (
                format!("config/{relative}"),
                config_path(manifest_dir, &relative),
            )
        })
        .collect()
}

/// `crate_dir` and every crate directory it reaches through `path = ...`
/// entries of the build and normal dependency tables, transitively, so a crate
/// added to a manifest is covered without a list to update. Canonical paths.
pub(super) fn path_dependency_dirs(crate_dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut found = Vec::new();
    collect_path_dependencies(crate_dir, &mut found)?;
    Ok(found)
}

fn collect_path_dependencies(dir: &Path, found: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    let dir = dir.canonicalize()?;
    if found.contains(&dir) {
        return Ok(());
    }
    found.push(dir.clone());
    let manifest: toml::Table = toml::from_str(&std::fs::read_to_string(dir.join("Cargo.toml"))?)?;
    let mut scopes = vec![&manifest];
    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        scopes.extend(targets.values().filter_map(toml::Value::as_table));
    }
    for scope in scopes {
        for key in ["dependencies", "build-dependencies"] {
            let Some(table) = scope.get(key).and_then(toml::Value::as_table) else {
                continue;
            };
            for path in table
                .values()
                .filter_map(|dependency| dependency.get("path")?.as_str())
            {
                collect_path_dependencies(&dir.join(path), found)?;
            }
        }
    }
    Ok(())
}

/// The files of `crate_dir` its non-test build can read: every file under
/// `src` and `build` outside a test-only module, then its build script and
/// manifest. A module declared under `#[cfg(test)]` (or
/// `#[cfg(all(test, ...))]`) never reaches the binary, so an edit there
/// rebuilds its test target only, not every crate linking this one. A file that
/// a `#[path]` attribute or an `include*!` literal names stays in: it may be
/// shared with a non-test build.
pub(super) fn crate_sources(crate_dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for source in ["src", "build"] {
        files_under(&crate_dir.join(source), &mut found)?;
    }
    let build_script = crate_dir.join("build.rs");
    if build_script.is_file() {
        found.push(build_script);
    }
    let mut test_only = HashSet::new();
    let mut named = HashSet::new();
    for file in found
        .iter()
        .filter(|file| file.extension().is_some_and(|ext| ext == "rs"))
    {
        let text = std::fs::read_to_string(file)?;
        let declared = declared_modules(file, &text);
        test_only.extend(declared.test_only);
        named.extend(declared.named);
        named.extend(named_files(file, &text));
    }
    found.retain(|file| {
        named.contains(file) || !file.ancestors().any(|path| test_only.contains(path))
    });
    let manifest = crate_dir.join("Cargo.toml");
    if manifest.is_file() {
        found.push(manifest);
    }
    Ok(found)
}

/// Every file under `dir`, recursively; a missing `dir` has none.
fn files_under(dir: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            files_under(&path, found)?;
        } else {
            found.push(path);
        }
    }
    Ok(())
}

/// What the top-level module declarations of one file say about other files.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Declared {
    /// The file and child directory of each test-only module: `mod name;`
    /// gives `name.rs` and `name/`, an inline `mod name { ... }` gives
    /// `name/`, where its out-of-line children live, and `#[path = "p"]`
    /// gives `p`.
    pub(super) test_only: Vec<PathBuf>,
    /// The files a non-test declaration mounts by `#[path = "p"]`: they stay
    /// in even inside a test-only module's directory.
    pub(super) named: Vec<PathBuf>,
}

/// The top-level module declarations of `file`; a `#[path]` is relative to
/// the file's directory.
pub(super) fn declared_modules(file: &Path, text: &str) -> Declared {
    let mut declared = Declared::default();
    let Some(dir) = file.parent() else {
        return declared;
    };
    let root = file
        .file_name()
        .is_some_and(|name| name == "lib.rs" || name == "main.rs" || name == "mod.rs");
    let children = match file.file_stem() {
        Some(stem) if !root => dir.join(stem),
        _ => dir.to_path_buf(),
    };
    let mut attributes: Vec<String> = Vec::new();
    // An attribute spanning lines, and its unclosed bracket depth.
    let mut open: Option<(String, i64)> = None;
    for line in text.lines() {
        if let Some((mut attribute, depth)) = open.take() {
            attribute.push_str(line.trim());
            let depth = depth + bracket_depth(line);
            if depth > 0 {
                open = Some((attribute, depth));
            } else {
                attributes.push(attribute);
            }
            continue;
        }
        // Top-level items start a line; an indented one is nested in an item.
        if line.starts_with(char::is_whitespace) || line.is_empty() || line.starts_with("//") {
            continue;
        }
        let mut rest = line.trim_end();
        while rest.starts_with("#[") {
            if let Some(end) = attribute_end(rest) {
                attributes.push(rest[..end].to_owned());
                rest = rest[end..].trim_start();
            } else {
                open = Some((rest.to_owned(), bracket_depth(rest)));
                rest = "";
            }
        }
        if rest.is_empty() {
            continue;
        }
        if let Some((name, inline)) = module_declaration(rest) {
            let test_only = attributes.iter().any(|attribute| is_test_cfg(attribute));
            let mounted = attributes
                .iter()
                .find_map(|attribute| attribute.strip_prefix("#[path = \""))
                .and_then(|path| path.split('"').next())
                .map(|path| resolved(&dir.join(path)));
            match (mounted, test_only) {
                (Some(path), true) => declared.test_only.push(path),
                (Some(path), false) => declared.named.push(path),
                (None, true) => {
                    if !inline {
                        declared.test_only.push(children.join(format!("{name}.rs")));
                    }
                    declared.test_only.push(children.join(name));
                }
                (None, false) => {}
            }
        }
        attributes.clear();
    }
    declared
}

/// `path` with `.` and `..` components resolved lexically.
fn resolved(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// Files `file` names by an `include!`, `include_str!` or `include_bytes!`
/// string literal, or by a `#[path = "..."]` attribute on an indented line (a
/// module nested in an item, which [`declared_modules`] does not read),
/// relative to its directory, with `..` resolved.
fn named_files(file: &Path, text: &str) -> Vec<PathBuf> {
    let Some(dir) = file.parent() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for marker in [
        "#[path = \"",
        "include!(\"",
        "include_str!(\"",
        "include_bytes!(\"",
    ] {
        for (start, _) in text.match_indices(marker) {
            let line_start = text[..start].rfind('\n').map_or(0, |newline| newline + 1);
            let indented = text[line_start..].starts_with(char::is_whitespace);
            if marker.starts_with("#[path") && !indented {
                continue;
            }
            let literal = &text[start + marker.len()..];
            if let Some(end) = literal.find('"') {
                found.push(resolved(&dir.join(&literal[..end])));
            }
        }
    }
    found
}

/// `[` minus `]` in `text`.
fn bracket_depth(text: &str) -> i64 {
    text.bytes().fold(0, |depth, byte| match byte {
        b'[' => depth + 1,
        b']' => depth - 1,
        _ => depth,
    })
}

/// The byte just past the `]` that closes the attribute `text` starts with.
fn attribute_end(text: &str) -> Option<usize> {
    let mut depth = 0i64;
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// The name of the module `line` declares, after any visibility, and whether
/// its body is inline (`mod name {`) rather than a file (`mod name;`).
fn module_declaration(line: &str) -> Option<(&str, bool)> {
    let mut rest = line;
    if let Some(after) = rest.strip_prefix("pub") {
        rest = after.trim_start();
        if rest.starts_with('(') {
            rest = rest[rest.find(')')? + 1..].trim_start();
        }
    }
    let rest = rest.strip_prefix("mod ")?.trim_start();
    let end = rest
        .find(|character: char| !(character.is_alphanumeric() || matches!(character, '_' | '#')))
        .unwrap_or(rest.len());
    let name = rest[..end].trim_start_matches("r#");
    if name.is_empty() {
        return None;
    }
    match rest[end..].trim_start().bytes().next() {
        Some(b';') => Some((name, false)),
        Some(b'{') => Some((name, true)),
        _ => None,
    }
}

/// Whether `attribute` is `#[cfg(test)]` or `#[cfg(all(..., test, ...))]`, a
/// condition only a test build meets.
fn is_test_cfg(attribute: &str) -> bool {
    let compact: String = attribute
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let Some(predicate) = compact
        .strip_prefix("#[cfg(")
        .and_then(|rest| rest.strip_suffix(")]"))
    else {
        return false;
    };
    let Some(terms) = predicate
        .strip_prefix("all(")
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return predicate == "test";
    };
    let mut depth = 0i64;
    let mut start = 0;
    let mut top_level = Vec::new();
    for (index, byte) in terms.bytes().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b',' if depth == 0 => {
                top_level.push(&terms[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    top_level.push(&terms[start..]);
    top_level.contains(&"test")
}
