//! The checked-out commit of a local root, read from the repository files
//! (`HEAD`, loose refs, `packed-refs`) without starting `git`.
//!
//! Local tool responses report it once, as `shared.commitSha`, so a caller
//! can confirm which commit the bytes came from. The working tree may hold
//! uncommitted edits; the value names HEAD, not a content hash.

use std::path::{Path, PathBuf};

/// Largest repository metadata file read (`HEAD`, a loose ref, a gitdir
/// pointer); `packed-refs` is scanned line by line up to [`MAX_PACKED_REFS`].
const MAX_SMALL_FILE: u64 = 4096;
const MAX_PACKED_REFS: u64 = 16 * 1024 * 1024;
/// Symbolic refs followed before giving up (a ref naming another ref).
const MAX_SYMBOLIC_HOPS: usize = 5;

/// The full HEAD commit SHA of the repository containing `root`, or `None`
/// outside a repository, on an unborn branch, or when the refs cannot be
/// read without `git` (for example a reftable store).
#[must_use]
pub fn head_sha(root: &Path) -> Option<String> {
    let start = if root.is_file() { root.parent()? } else { root };
    let git_dir = start.ancestors().find_map(git_dir_at)?;
    let common = common_dir(&git_dir);
    let mut reference = read_small(&git_dir.join("HEAD"))?;
    for _ in 0..MAX_SYMBOLIC_HOPS {
        let value = reference.trim();
        let Some(name) = value.strip_prefix("ref:") else {
            return is_sha(value).then(|| value.to_ascii_lowercase());
        };
        let name = name.trim();
        if !is_ref_name(name) {
            return None;
        }
        // Per-worktree refs live in the worktree's git dir; branches and
        // tags in the common dir.
        reference = read_small(&git_dir.join(name))
            .or_else(|| read_small(&common.join(name)))
            .or_else(|| packed_ref(&common, name))?;
    }
    None
}

/// The git directory of a working-tree directory: `.git` itself, or the
/// `gitdir:` a `.git` file points at (worktrees, submodules).
fn git_dir_at(dir: &Path) -> Option<PathBuf> {
    let dot_git = dir.join(".git");
    let meta = std::fs::metadata(&dot_git).ok()?;
    if meta.is_dir() {
        return dot_git.join("HEAD").is_file().then_some(dot_git);
    }
    let pointer = read_small(&dot_git)?;
    let target = pointer.trim().strip_prefix("gitdir:")?.trim();
    let target = dir.join(target);
    target.join("HEAD").is_file().then_some(target)
}

/// The shared repository directory of a linked worktree (`commondir`), or
/// the git dir itself.
fn common_dir(git_dir: &Path) -> PathBuf {
    read_small(&git_dir.join("commondir"))
        .map(|relative| git_dir.join(relative.trim()))
        .filter(|common| common.is_dir())
        .unwrap_or_else(|| git_dir.to_path_buf())
}

fn packed_ref(common: &Path, name: &str) -> Option<String> {
    use std::io::{BufRead, BufReader, Read};
    let file = std::fs::File::open(common.join("packed-refs")).ok()?;
    let reader = BufReader::new(file.take(MAX_PACKED_REFS));
    reader.lines().map_while(Result::ok).find_map(|line| {
        let (sha, reference) = line.split_once(' ')?;
        (reference.trim() == name && is_sha(sha)).then(|| sha.to_owned())
    })
}

fn read_small(path: &Path) -> Option<String> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut text = String::new();
    file.take(MAX_SMALL_FILE).read_to_string(&mut text).ok()?;
    Some(text)
}

/// A SHA-1 or SHA-256 object name.
fn is_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// A ref path under the git dir that cannot escape it.
fn is_ref_name(name: &str) -> bool {
    name.starts_with("refs/")
        && !name.contains("..")
        && !name.contains('\\')
        && Path::new(name)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::head_sha;
    use std::fs;

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
    const OTHER: &str = "89abcdef0123456789abcdef0123456789abcdef";

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join(".git/refs/heads")).expect("refs");
        fs::create_dir_all(dir.path().join("src/deep")).expect("src");
        fs::write(dir.path().join("src/deep/a.rs"), "fn a() {}\n").expect("file");
        dir
    }

    #[test]
    fn branch_head_resolves_through_a_loose_ref_from_any_subpath() {
        let dir = repo();
        fs::write(dir.path().join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(dir.path().join(".git/refs/heads/main"), format!("{SHA}\n")).unwrap();
        assert_eq!(head_sha(dir.path()).as_deref(), Some(SHA));
        assert_eq!(head_sha(&dir.path().join("src/deep")).as_deref(), Some(SHA));
        assert_eq!(
            head_sha(&dir.path().join("src/deep/a.rs")).as_deref(),
            Some(SHA)
        );
    }

    #[test]
    fn packed_refs_and_detached_heads_resolve() {
        let dir = repo();
        fs::write(dir.path().join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(
            dir.path().join(".git/packed-refs"),
            format!("# pack-refs with: peeled fully-peeled sorted\n{OTHER} refs/heads/dev\n{SHA} refs/heads/main\n^{OTHER}\n"),
        )
        .unwrap();
        assert_eq!(head_sha(dir.path()).as_deref(), Some(SHA));
        fs::write(dir.path().join(".git/HEAD"), format!("{OTHER}\n")).unwrap();
        assert_eq!(head_sha(dir.path()).as_deref(), Some(OTHER));
    }

    #[test]
    fn linked_worktrees_read_branches_from_the_common_dir() {
        let dir = repo();
        let main = dir.path().join(".git");
        fs::write(main.join("refs/heads/feature"), format!("{SHA}\n")).unwrap();
        let worktree_git = main.join("worktrees/wt");
        fs::create_dir_all(&worktree_git).unwrap();
        fs::write(worktree_git.join("HEAD"), "ref: refs/heads/feature\n").unwrap();
        fs::write(worktree_git.join("commondir"), "../..\n").unwrap();
        let checkout = dir.path().join("checkout");
        fs::create_dir_all(&checkout).unwrap();
        fs::write(
            checkout.join(".git"),
            format!("gitdir: {}\n", worktree_git.display()),
        )
        .unwrap();
        assert_eq!(head_sha(&checkout).as_deref(), Some(SHA));
    }

    #[test]
    fn unborn_escaping_and_non_repository_roots_report_nothing() {
        let dir = repo();
        fs::write(dir.path().join(".git/HEAD"), "ref: refs/heads/none\n").unwrap();
        assert_eq!(head_sha(dir.path()), None);
        fs::write(
            dir.path().join(".git/HEAD"),
            "ref: refs/../../../etc/passwd\n",
        )
        .unwrap();
        assert_eq!(head_sha(dir.path()), None);
        fs::write(dir.path().join(".git/HEAD"), "not a sha\n").unwrap();
        assert_eq!(head_sha(dir.path()), None);
        let plain = tempfile::tempdir().expect("tempdir");
        assert_eq!(head_sha(plain.path()), None);
    }
}
