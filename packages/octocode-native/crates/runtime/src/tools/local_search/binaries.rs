//! Binary files a search skipped or cut: their warnings, groups and listings.

use super::types::*;
use crate::id::ToolId;
use crate::policy::prune::DefaultsFlag;
use crate::response::result::Continuation;
use serde_json::{Value, json};

/// `binarySkipped` warning: how many files were skipped as binary from
/// their leading bytes, grouped by extension (`.woff2 54, .png 2`).
pub(super) fn leading_binary_warning(
    groups: &[octocode_engine::types::BinaryExtensionCount],
) -> String {
    let total: u64 = groups.iter().map(|group| u64::from(group.count)).sum();
    let by_extension = binary_counts(groups);
    let noun = if total == 1 {
        "binary file"
    } else {
        "binary files"
    };
    format!("binarySkipped: {total} {noun} not searched ({by_extension}).")
}

/// Most chars of exact names a binary listing's `nameRegex` spells out.
pub(super) const MAX_LISTED_NAME_CHARS: usize = 2_000;

/// Most extensions a structureSearch `extensions` filter takes.
pub(super) const MAX_LISTED_EXTENSIONS: usize = 100;

/// A structureSearch `files` query over the searched scope that lists every
/// skipped binary: by `extensions`, or by a basename regex when one has no
/// extension (or there are more extensions than the filter takes). It may
/// also list same-extension text files; it never misses a skipped one.
pub(super) fn leading_binary_listing(
    query: &LocalSearchQuery,
    groups: &[octocode_engine::types::BinaryExtensionCount],
) -> Value {
    let mut listing = json!({
        "operation": "files",
        "path": query.path.as_str(),
        "entryType": "f",
    });
    let by_extension = groups.len() <= MAX_LISTED_EXTENSIONS
        && groups.iter().all(|group| !group.extension.is_empty());
    if by_extension {
        listing["extensions"] = json!(
            groups
                .iter()
                .map(|group| group.extension.as_str())
                .collect::<Vec<_>>()
        );
    } else {
        // Extensionless binaries by exact name: a no-extension pattern
        // would also list every extensionless text file.
        let names = groups
            .iter()
            .filter(|group| group.extension.is_empty())
            .flat_map(|group| group.names.iter().map(|name| regex::escape(name)))
            .collect::<Vec<_>>();
        let extensions = groups
            .iter()
            .filter(|group| !group.extension.is_empty())
            .map(|group| regex::escape(&group.extension))
            .collect::<Vec<_>>();
        let mut alternatives = Vec::new();
        if !names.is_empty() {
            let exact = format!("^(?:{})$", names.join("|"));
            // Too many names for one call: every extensionless name instead,
            // which may also list text files but never misses a binary.
            alternatives.push(if exact.len() > MAX_LISTED_NAME_CHARS {
                r"^\.?[^.]*$".to_owned()
            } else {
                exact
            });
        }
        if !extensions.is_empty() {
            alternatives.push(format!(r"\.(?i:{})$", extensions.join("|")));
        }
        listing["nameRegex"] = json!(alternatives.join("|"));
    }
    if let Some(depth) = query.max_depth {
        listing["maxDepth"] = json!(depth);
    }
    if !query.exclude.is_empty() {
        listing["exclude"] = json!(query.exclude);
    }
    if !query.default_excludes.defaults() {
        listing["defaultExcludes"] = json!(false);
    }
    if query.include_ignored == Some(true) {
        listing["includeIgnored"] = json!(true);
    }
    Continuation::new(ToolId::StructureSearch, listing).build()
}

/// Binary-cut files a warning names one by one; more are a count by
/// extension, listed whole by the `binarySkipped` lead.
pub(super) const MAX_NAMED_BINARY_FILES: usize = 10;

/// Paths grouped by lowercased extension (`""` for none, naming those
/// files), most files first, as the engine groups skipped binaries.
pub(super) fn binary_groups(paths: &[String]) -> Vec<octocode_engine::types::BinaryExtensionCount> {
    let mut groups = std::collections::BTreeMap::<String, (u32, Vec<String>)>::new();
    for path in paths {
        let path = std::path::Path::new(path);
        let extension = octocode_engine::text::extension_of(&path.to_string_lossy(), true, "");
        let group = groups.entry(extension.clone()).or_default();
        group.0 = group.0.saturating_add(1);
        if extension.is_empty()
            && let Some(name) = path.file_name()
        {
            group.1.push(name.to_string_lossy().into_owned());
        }
    }
    let mut groups = groups
        .into_iter()
        .map(|(extension, (count, mut names))| {
            names.sort();
            names.dedup();
            octocode_engine::types::BinaryExtensionCount {
                extension,
                count,
                names,
            }
        })
        .collect::<Vec<_>>();
    groups.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.extension.cmp(&b.extension))
    });
    groups
}

/// Two extension groupings as one: counts summed, names joined.
pub(super) fn merge_binary_groups(
    first: &[octocode_engine::types::BinaryExtensionCount],
    second: &[octocode_engine::types::BinaryExtensionCount],
) -> Vec<octocode_engine::types::BinaryExtensionCount> {
    let mut merged: Vec<octocode_engine::types::BinaryExtensionCount> = first.to_vec();
    for group in second {
        match merged
            .iter_mut()
            .find(|known| known.extension == group.extension)
        {
            Some(known) => {
                known.count = known.count.saturating_add(group.count);
                known.names.extend(group.names.iter().cloned());
                known.names.sort();
                known.names.dedup();
            }
            None => merged.push(group.clone()),
        }
    }
    merged
}

/// `.woff2 54, .png 2, no extension 1`.
pub(super) fn binary_counts(groups: &[octocode_engine::types::BinaryExtensionCount]) -> String {
    groups
        .iter()
        .map(|group| {
            if group.extension.is_empty() {
                format!("no extension {}", group.count)
            } else {
                format!(".{} {}", group.extension, group.count)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The binary-cut files a warning names: every root-relative path, files
/// sharing a directory written once under it (`dir/{a.txt,b.txt}`), then
/// the count of any the engine could not name (`and 3 more`). Past
/// [`MAX_NAMED_BINARY_FILES`] they are a count by extension, and the
/// `binarySkipped` listing names each one.
pub(super) fn binary_file_list(paths: &[String], total: u32, root: &std::path::Path) -> String {
    if paths.is_empty() {
        return "a file with a NUL byte was".into();
    }
    if paths.len() > MAX_NAMED_BINARY_FILES {
        let total = (total as usize).max(paths.len());
        return format!(
            "{total} files ({}) were",
            binary_counts(&binary_groups(paths))
        );
    }
    let mut by_dir = std::collections::BTreeMap::<String, Vec<String>>::new();
    for path in paths {
        let path = std::path::Path::new(path);
        let relative = path
            .strip_prefix(root)
            .ok()
            .filter(|relative| !relative.as_os_str().is_empty())
            .unwrap_or(path);
        let dir = relative
            .parent()
            .map(|dir| dir.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = relative.file_name().map_or_else(
            || relative.to_string_lossy().into_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        by_dir.entry(dir).or_default().push(name);
    }
    let mut names = by_dir
        .into_iter()
        .flat_map(|(dir, names)| {
            let prefix = if dir.is_empty() {
                String::new()
            } else {
                format!("{dir}/")
            };
            if names.len() == 1 || dir.is_empty() {
                names
                    .into_iter()
                    .map(|name| format!("{prefix}{name}"))
                    .collect::<Vec<_>>()
            } else {
                vec![format!("{prefix}{{{}}}", names.join(","))]
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let unnamed = (total as usize).saturating_sub(paths.len());
    if unnamed > 0 {
        names.push_str(&format!(" and {unnamed} more"));
    }
    let verb = if paths.len() + unnamed == 1 {
        "was"
    } else {
        "were"
    };
    format!("{names} {verb}")
}
