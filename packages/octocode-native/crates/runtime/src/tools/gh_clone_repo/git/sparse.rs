//! `sparse-checkout set` arguments: cone mode for directories, exact anchored patterns once any path is a file.
use super::{run, scoped, sparse_timeout};
use crate::tools::gh_clone_repo::{CloneContext, CloneError};
use std::ffi::OsString;
use std::path::Path;

/// `sparse-checkout set` arguments for `sparse_paths` at the fetched commit. Cone mode
/// always includes every file beside each cone directory, so when any path is
/// a file every path becomes an exact anchored non-cone pattern (a directory
/// as `/dir/`); directories alone (and paths that do not resolve, left for
/// the caller's not-found check) keep cone mode. A failed `ls-tree` fails
/// the checkout: guessing cone mode would widen a file scope to its folder.
pub(super) fn sparse_set_args(
    context: &CloneContext<'_>,
    target: &Path,
    sparse_paths: &[String],
) -> Result<Vec<OsString>, CloneError> {
    let paths = sparse_paths
        .iter()
        .map(|path| path.trim_end_matches('/'))
        .collect::<Vec<_>>();
    // ls-tree reads tree objects only, so it never lazily fetches a blob;
    // `-z` keeps unusual paths unquoted.
    let mut ls_tree = scoped(target, &["ls-tree", "-z", "FETCH_HEAD", "--"]);
    ls_tree.extend(paths.iter().map(OsString::from));
    let files = run(
        context,
        ls_tree,
        sparse_timeout(context),
        "resolve sparse path type",
        None,
    )?
    .stdout
    .split('\0')
    .filter_map(|line| {
        let (meta, entry) = line.split_once('\t')?;
        (meta.split_whitespace().nth(1) == Some("blob")).then(|| entry.to_owned())
    })
    .collect::<Vec<_>>();
    let is_file = |path: &str| files.iter().any(|file| file == path);
    if !paths.iter().any(|path| is_file(path)) {
        let mut args = scoped(
            target,
            &["sparse-checkout", "set", "--cone", "--skip-checks", "--"],
        );
        args.extend(sparse_paths.iter().map(OsString::from));
        return Ok(args);
    }
    let mut args = scoped(target, &["sparse-checkout", "set", "--no-cone", "--"]);
    args.extend(paths.iter().map(|path| {
        let mut pattern = file_pattern(path);
        if !is_file(path) {
            pattern.push('/');
        }
        OsString::from(pattern)
    }));
    Ok(args)
}

/// Anchored gitignore-style pattern matching exactly one repo-relative file.
fn file_pattern(path: &str) -> String {
    let mut pattern = String::from("/");
    for character in path.chars() {
        if matches!(character, '\\' | '*' | '?' | '[' | ']' | '!' | '#') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern
}
