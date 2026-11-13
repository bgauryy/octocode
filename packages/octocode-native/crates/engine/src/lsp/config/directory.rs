//! A directory-path query's language server: the languages of the
//! directory's files, else of its project markers, decide it.
use super::catalog::{default_server_for_file, typescript_server_fallback};
use super::types::LspDiscoveryOptions;
use super::workspace_root::{
    SKIPPED_DIRS, SOURCE_FAMILIES, representative_source_for, workspace_root_languages,
};
use crate::lsp::types::LspLanguageServerConfig;
use crate::text::extension_of;
use std::path::Path;

/// Resolve the language server for a directory (the directory-path query
/// shape) searched from `workspace_root`: the server of the file
/// [`directory_probe_path`] names, routed as a file-anchored call on it would
/// be. Returns `None` when neither the directory nor the root implies a
/// language.
pub fn default_server_for_directory(
    directory: &str,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Option<LspLanguageServerConfig> {
    let probe = directory_probe_path(directory, workspace_root)?;
    default_server_for_file(&probe, workspace_root, options)
}

/// [`typescript_server_fallback`] for a directory-path query.
pub fn typescript_server_fallback_for_directory(
    directory: &str,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
) -> Option<LspLanguageServerConfig> {
    let probe = directory_probe_path(directory, workspace_root)?;
    typescript_server_fallback(&probe, workspace_root, options)
}

/// The file a directory routes by: its representative source
/// ([`directory_representative`]), else a synthetic file in the language
/// [`directory_languages`] puts first (discovery keys only on its extension;
/// it is never opened or synced).
fn directory_probe_path(directory: &str, workspace_root: &str) -> Option<String> {
    directory_representative(directory, workspace_root).or_else(|| {
        let extension = directory_languages(directory, workspace_root)
            .into_iter()
            .next()?;
        Some(
            Path::new(workspace_root)
                .join(format!("workspace{extension}"))
                .to_string_lossy()
                .into_owned(),
        )
    })
}

/// The source file a directory-path query opens so the server loads a
/// project, in the language [`directory_languages`] puts first: one inside
/// the directory, else one in the workspace root (a directory of headers
/// or scripts the root's project compiles from elsewhere).
#[must_use]
pub fn directory_representative(directory: &str, workspace_root: &str) -> Option<String> {
    let extension = directory_languages(directory, workspace_root)
        .into_iter()
        .next()?;
    representative_source_for(directory, extension)
        .or_else(|| representative_source_for(workspace_root, extension))
}

/// The languages of a directory as representative-source extensions
/// (`.ts`, `.py`, ...), the one that routes it first, each once:
///
/// 1. the directory's own project markers ([`workspace_root_languages`]):
///    it is a project root;
/// 2. else the languages of its source files, most files first: its own
///    files, or those of the first level below it that holds any (a `src`
///    of packages). A folder of Python scripts under a TypeScript
///    workspace is Python's, and a package directory with no manifest of
///    its own is its files' language;
/// 3. else the markers of `workspace_root`, the project it is searched
///    from (a directory with no source file of its own).
///
/// The languages after the first are other sources the directory holds,
/// which a directory-path query does not search.
#[must_use]
pub fn directory_languages(directory: &str, workspace_root: &str) -> Vec<&'static str> {
    let own = workspace_root_languages(directory);
    if !own.is_empty() {
        return own;
    }
    let files = file_languages(Path::new(directory));
    if !files.is_empty() {
        return files;
    }
    workspace_root_languages(workspace_root)
}

/// The languages of the source files at the first level of `directory`'s
/// tree that holds any, most files first (ties in [`SOURCE_FAMILIES`]
/// order). The walk skips the directories [`first_source_under`] skips and
/// ends at that level, so its cost is the tree above the first sources.
fn file_languages(directory: &Path) -> Vec<&'static str> {
    let mut level = vec![directory.to_path_buf()];
    while !level.is_empty() {
        let mut counts = vec![0usize; SOURCE_FAMILIES.len()];
        let mut below = Vec::new();
        for dir in level {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                let name = entry.file_name().to_string_lossy().into_owned();
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_dir() {
                    if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_str()) {
                        below.push(entry.path());
                    }
                } else if file_type.is_file()
                    && let Some(family) = family_of_file(&name)
                {
                    counts[family] += 1;
                }
            }
        }
        if counts.iter().any(|count| *count > 0) {
            let mut ranked = (0..SOURCE_FAMILIES.len())
                .filter(|family| counts[*family] > 0)
                .collect::<Vec<_>>();
            ranked.sort_by_key(|family| std::cmp::Reverse(counts[*family]));
            return ranked
                .into_iter()
                .map(|family| SOURCE_FAMILIES[family].0)
                .collect();
        }
        level = below;
    }
    Vec::new()
}

/// The index in [`SOURCE_FAMILIES`] of the family a file name belongs to.
fn family_of_file(name: &str) -> Option<usize> {
    let extension = extension_of(name, true, "");
    SOURCE_FAMILIES
        .iter()
        .position(|(_, extensions)| extensions.contains(&extension.as_str()))
}
