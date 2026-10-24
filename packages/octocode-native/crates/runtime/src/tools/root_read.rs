//! What a directory listing shares between ghStructure (a remote listing)
//! and structureSearch (a local one): the file it leads to first, so one tree
//! gets the same read lead from either tool, and the text of a listed entry.
use crate::content::{FileType, classify_file_type};
use octocode_engine::text::test_paths::is_test_path;

/// Rank of a file no [`read_rank`] tier names (an outline-able file, or one
/// in a small listing): it reads last.
pub(crate) const RANK_ANY: u8 = 6;

/// One listed entry: its `name`, `/` after a directory, then ` (<fields>)`
/// when it has `fields`. The fields never contain ` (`, so the last ` (` of an
/// entry ending in `)` opens them, whatever the name holds.
pub(crate) fn entry_text(name: &str, is_dir: bool, fields: &[String]) -> String {
    let mut text = name.to_owned();
    if is_dir {
        text.push('/');
    }
    if !fields.is_empty() {
        text.push_str(&format!(" ({})", fields.join(", ")));
    }
    text
}

/// A listed file's read rank (lower reads first), or `None` for a file no
/// lead should read (tests, data, config, generated output).
///
/// `depth` is how many directories the file sits below the listed one. Docs
/// and manifests describe a directory only when they sit directly in it:
/// - 0: a non-test source file whose stem a bare-word `include` names
///   (`words`, lowercased): the file the caller asked for
/// - 1: a code entry file (package init, index, module root, main), at any
///   depth
/// - 2: a package manifest
/// - 3: the README
/// - 4: an `AGENTS.md` or `ARCHITECTURE.md` guide
/// - 5: other non-test source, at any depth: a loose script reads last
pub(crate) fn read_rank(path: &str, depth: usize, words: &[String]) -> Option<u8> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let lower = name.to_ascii_lowercase();
    let code = matches!(classify_file_type(path), Some(FileType::Code)) && !is_test_path(path);
    let stem = lower
        .rsplit_once('.')
        .map_or(lower.as_str(), |(stem, _)| stem);
    if code && words.iter().any(|word| word == stem) {
        return Some(0);
    }
    if code
        && CODE_ENTRY_FILES
            .iter()
            .any(|wanted| entry_matches(&lower, wanted))
    {
        return Some(1);
    }
    if depth == 0 {
        if MANIFEST_FILES.contains(&lower.as_str()) {
            return Some(2);
        }
        if lower.starts_with("readme") {
            return Some(3);
        }
        if GUIDE_FILES.contains(&lower.as_str()) {
            return Some(4);
        }
    }
    code.then_some(5)
}

/// Guides written for whoever starts reading the project.
const GUIDE_FILES: &[&str] = &["agents.md", "architecture.md"];

/// Package manifests.
const MANIFEST_FILES: &[&str] = &[
    "package.json",
    "cargo.toml",
    "pyproject.toml",
    "go.mod",
    "pom.xml",
];

/// Code entry files: package init, index, module root, main.
const CODE_ENTRY_FILES: &[&str] = &["__init__.py", "index.*", "mod.rs", "lib.rs", "main.*"];

/// Whether a lowercased file name is `wanted`, a name or a `stem.*` pattern.
fn entry_matches(lower: &str, wanted: &str) -> bool {
    match wanted.strip_suffix('*') {
        Some(stem) => lower.starts_with(stem),
        None => lower == wanted,
    }
}

#[cfg(test)]
mod tests {
    use super::{entry_text, read_rank};

    fn rank(path: &str) -> Option<u8> {
        read_rank(path, path.matches('/').count(), &[])
    }

    /// A project root reads its manifest, README and guides before a loose
    /// script (zod's `mm.mjs`, meilisearch's `download-latest.sh`).
    #[test]
    fn a_roots_docs_and_manifest_outrank_loose_source() {
        let order = [
            "index.ts",
            "package.json",
            "README.md",
            "AGENTS.md",
            "mm.mjs",
        ];
        let ranks: Vec<_> = order.iter().map(|path| rank(path)).collect();
        assert!(ranks.windows(2).all(|pair| pair[0] < pair[1]), "{ranks:?}");
        assert!(rank("download-latest.sh") > rank("ARCHITECTURE.md"));
    }

    /// Only the listed directory's own docs describe it: a nested README
    /// does not outrank source.
    #[test]
    fn a_nested_readme_or_manifest_does_not_outrank_source() {
        assert_eq!(rank("examples/app/README.md"), None);
        assert_eq!(rank("packages/x/package.json"), None);
        assert_eq!(rank("src/lib.rs"), Some(1));
        assert_eq!(rank("src/util.rs"), Some(5));
    }

    #[test]
    fn tests_and_data_are_not_read_leads() {
        assert_eq!(rank("src/a.test.ts"), None);
        assert_eq!(rank("data.json"), None);
    }

    #[test]
    fn an_entry_names_a_directory_with_a_slash_and_lists_its_fields() {
        assert_eq!(entry_text("a.rs", false, &[]), "a.rs");
        assert_eq!(entry_text("src", true, &[]), "src/");
        let fields = ["12".to_owned(), "symlink".to_owned()];
        assert_eq!(entry_text("a.rs", false, &fields), "a.rs (12, symlink)");
        assert_eq!(entry_text("src", true, &fields[..1]), "src/ (12)");
    }

    #[test]
    fn a_bare_word_names_its_source_file_first() {
        let words = ["parse".to_owned()];
        assert_eq!(read_rank("src/parse.ts", 1, &words), Some(0));
        assert_eq!(read_rank("README.md", 0, &words), Some(3));
    }
}
