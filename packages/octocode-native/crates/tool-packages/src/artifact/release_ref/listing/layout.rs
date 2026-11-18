//! Whether a tag whose prefix has a package's shape (`<stem>-v`, a path) is
//! another package's: the repository's own layout at the tag's commit
//! says, a directory named for the stem holding a package manifest.
use super::{Candidate, CheckFailure, ReleaseTag, ReleaseTags, TreeFiles, key};

/// The manifest file names a package of each ecosystem artifactSearch
/// resolves is built from (lowercased): npm, PyPI, crates.io, Go, Maven and
/// Gradle, Packagist; RubyGems and NuGet by their extensions.
const PACKAGE_MANIFESTS: &[&str] = &[
    "package.json",
    "pyproject.toml",
    "setup.py",
    "setup.cfg",
    "cargo.toml",
    "go.mod",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "composer.json",
];
const PACKAGE_MANIFEST_EXTENSIONS: &[&str] = &[".gemspec", ".nuspec", ".csproj", ".fsproj"];

/// Whether the tree's `files` hold a package `stem`: a directory whose leaf
/// keys to the stem's leaf holds a package manifest. The repository root is
/// no other package.
fn holds_package(files: &[String], stem: &str) -> bool {
    let named = key(leaf(stem));
    files.iter().any(|path| {
        let Some((directory, name)) = path.rsplit_once('/') else {
            return false;
        };
        let name = name.to_ascii_lowercase();
        let manifest = PACKAGE_MANIFESTS.contains(&name.as_str())
            || PACKAGE_MANIFEST_EXTENSIONS
                .iter()
                .any(|extension| name.ends_with(extension));
        manifest && key(leaf(directory)) == named
    })
}

/// The last segment of a path (all of a name with no `/`).
fn leaf(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// A stem tag no package of the repository accounts for: its tag, its
/// stem, and what the layout check found.
pub(super) struct Unattested {
    pub(super) tag: String,
    pub(super) stem: String,
    pub(super) checked: String,
}

/// Sorts `candidates` (in pin order) by the repository's layout at each stem
/// tag's commit, one tree read per distinct commit (GitHub's request
/// admission bounds the reads): a stem the tree holds as a package is
/// another package's tag (`foreign`); every other candidate may be this
/// package's release, in order, and each unattested stem says what was
/// checked.
pub(super) async fn attest_stems(
    tags: &dyn ReleaseTags,
    owner: &str,
    repo: &str,
    candidates: Vec<Candidate>,
    foreign: &mut Vec<String>,
) -> (Vec<ReleaseTag>, Vec<Unattested>) {
    let mut commits: Vec<&str> = candidates
        .iter()
        .filter(|candidate| candidate.stem.is_some())
        .map(|candidate| candidate.release.commit.as_str())
        .collect();
    commits.sort_unstable();
    commits.dedup();
    let trees = futures_util::future::join_all(
        commits
            .iter()
            .map(|commit| tags.tree_files(owner, repo, commit)),
    )
    .await;
    let layouts: Vec<(String, Result<TreeFiles, CheckFailure>)> =
        commits.into_iter().map(str::to_owned).zip(trees).collect();
    let mut releases = Vec::new();
    let mut unattested = Vec::new();
    for Candidate { release, stem } in candidates {
        let Some(stem) = stem else {
            releases.push(release);
            continue;
        };
        let commit = release.commit.as_str();
        let named = leaf(&stem);
        // Every stem tag's commit was read above.
        let checked = match layouts.iter().find(|(read, _)| read == commit) {
            Some((_, Ok(tree))) if holds_package(&tree.paths, &stem) => {
                foreign.push(release.tag);
                continue;
            }
            Some((_, Ok(tree))) if tree.truncated => format!(
                "the part of the repository's tree at commit {commit} that GitHub returns (its recursive-tree limit cut the rest) has no directory named {named} holding a package manifest"
            ),
            Some((_, Ok(_))) => format!(
                "the repository at commit {commit} has no directory named {named} holding a package manifest"
            ),
            Some((_, Err(cause))) => format!(
                "whether the repository holds a package {named} could not be read ({})",
                cause.describe()
            ),
            None => format!("the repository's tree at commit {commit} was not read"),
        };
        unattested.push(Unattested {
            tag: release.tag.clone(),
            stem,
            checked,
        });
        releases.push(release);
    }
    (releases, unattested)
}
