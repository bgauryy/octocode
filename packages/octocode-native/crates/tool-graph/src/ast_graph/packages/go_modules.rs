//! Go modules that are local code though the scan does not hold them: what a
//! `replace` to a local path or a `go.work` member names. An import of one is
//! an unscanned part of the repository, not a third-party module.
use super::go_module_path;
use octocode_engine::text::lexical;
use std::path::{Path, PathBuf};

/// The entries of `keyword` in a `go.mod` or `go.work`, one-line or in a block.
fn directive_entries<'t>(text: &'t str, keyword: &str) -> Vec<&'t str> {
    let mut entries = Vec::new();
    let mut in_block = false;
    for line in text.lines().map(str::trim) {
        if in_block {
            if line == ")" {
                in_block = false;
            } else if !line.is_empty() && !line.starts_with("//") {
                entries.push(line);
            }
        } else if let Some(rest) = line.strip_prefix(keyword) {
            match rest.trim() {
                "(" => in_block = true,
                "" => {}
                entry if rest.starts_with(char::is_whitespace) => entries.push(entry),
                _ => {}
            }
        }
    }
    entries
}

/// Whether the directory `path` names from `dir` lies outside `root`.
fn outside(root: &Path, dir: &Path, path: &str) -> bool {
    !lexical(&dir.join(path)).starts_with(root)
}

/// The modules the `go.mod` in `dir` replaces with a local directory, each with
/// whether that directory lies outside the scan `root`.
pub(super) fn local_replacements(text: &str, dir: &Path, root: &Path) -> Vec<(String, bool)> {
    directive_entries(text, "replace")
        .into_iter()
        .filter_map(|entry| {
            let (old, new) = entry.split_once("=>")?;
            let path = new.split_whitespace().next()?;
            path.starts_with(['.', '/']).then(|| {
                old.split_whitespace()
                    .next()
                    .map(|module| (module.to_owned(), outside(root, dir, path)))
            })?
        })
        .collect()
}

/// The module paths of the `go.work` members at or above `root`, each with
/// whether its directory lies outside `root`.
pub(super) fn workspace_members(
    root: &Path,
    above: &[PathBuf],
    read: &dyn Fn(&Path) -> Option<String>,
) -> Vec<(String, bool)> {
    let work = std::iter::once(root)
        .chain(above.iter().map(PathBuf::as_path))
        .find_map(|dir| Some((dir, read(&dir.join("go.work"))?)));
    let Some((dir, text)) = work else {
        return Vec::new();
    };
    directive_entries(&text, "use")
        .into_iter()
        .filter_map(|member| {
            let member = member.split("//").next()?.trim().trim_matches('"');
            go_module_path(&read(&dir.join(member).join("go.mod"))?)
                .map(|module| (module, outside(root, dir, member)))
        })
        .collect()
}

#[cfg(test)]
mod tests;
