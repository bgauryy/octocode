//! Binary files a search skipped or cut: their warnings, groups and listings.
use crate::tools::local_search::local_search_query;

use crate::policy::prune::DefaultsFlag;
use crate::response::result::Continuation;
use crate::tools::leads::files_listing::files_listing;
use octocode_contract_types::{
    DefaultExcludes, LocalSearchQuery, StructureSearchQuery, StructureSearchQueryFiles,
    StructureSearchQueryFilesEntryType,
};
use serde_json::Value;
use std::num::NonZeroU64;

/// `binarySkipped` warning: how many files were skipped as binary from
/// their leading bytes, grouped by extension (`54 .woff2, 2 .png`).
pub(super) fn leading_binary_warning(
    groups: &[octocode_engine::types::BinaryExtensionCount],
) -> String {
    let total: u64 = groups.iter().map(|group| u64::from(group.count)).sum();
    let total = octocode_engine::text::counted(total, "binary file", "binary files");
    format!(
        "binarySkipped: {total} not searched ({}).",
        binary_counts(groups)
    )
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
) -> Option<Value> {
    let by_extension = groups.len() <= MAX_LISTED_EXTENSIONS
        && groups.iter().all(|group| !group.extension.is_empty());
    let mut extensions = Vec::new();
    let mut name_regex = None;
    if by_extension {
        for group in groups {
            extensions.push(group.extension.as_str().try_into().ok()?);
        }
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
        name_regex = Some(alternatives.join("|").try_into().ok()?);
    }
    let listing = StructureSearchQueryFiles {
        default_excludes: (!local_search_query::default_excludes(query).defaults())
            .then_some(DefaultExcludes(false)),
        entry_type: Some(StructureSearchQueryFilesEntryType::File),
        exclude: local_search_query::exclude(query).to_vec(),
        extensions,
        include_hidden: (local_search_query::include_hidden(query) == Some(true)).then_some(true),
        include_ignored: (local_search_query::include_ignored(query) == Some(true)).then_some(true),
        max_depth: local_search_query::max_depth(query)
            .and_then(|depth| NonZeroU64::new(u64::from(depth.saturating_add(1)))),
        name_regex,
        ..files_listing(local_search_query::path(query))?
    };
    Some(Continuation::related_query(StructureSearchQuery::from(listing)).build())
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

/// The extensions of counted files: `54 .woff2, 2 .png, 1 with no
/// extension`. One group needs no count, its total is said beside it:
/// `.png`, `all .rlib`, `no extension`.
pub(super) fn binary_counts(groups: &[octocode_engine::types::BinaryExtensionCount]) -> String {
    let label = |group: &octocode_engine::types::BinaryExtensionCount| {
        if group.extension.is_empty() {
            "no extension".to_owned()
        } else {
            format!(".{}", group.extension)
        }
    };
    if let [group] = groups {
        return if group.count > 1 && !group.extension.is_empty() {
            format!("all {}", label(group))
        } else {
            label(group)
        };
    }
    groups
        .iter()
        .map(|group| {
            if group.extension.is_empty() {
                format!("{} with no extension", group.count)
            } else {
                format!("{} {}", group.count, label(group))
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
