//! The one reading of an `include` entry every tool shares: a glob as
//! written, or a bare word (no glob metacharacter) that matches names
//! containing it and everything under a directory whose name contains it.

/// The glob an `include` entry stands for: a bare word (no `/`, no glob
/// metacharacter) becomes `*word*`, names containing it at any depth; any
/// other entry is a glob as written, with a leading `./` dropped.
pub(crate) fn include_glob(pattern: &str) -> String {
    let pattern = pattern.trim().trim_start_matches("./");
    if is_bare_word(pattern) {
        format!("*{pattern}*")
    } else {
        pattern.to_owned()
    }
}

const GLOB_META: [char; 4] = ['*', '?', '[', '{'];

fn is_bare_word(pattern: &str) -> bool {
    !pattern.contains('/') && !pattern.contains(GLOB_META)
}

/// Every `include` entry as a glob. A path without glob metacharacters
/// (`src/api`, `src/api/x.ts`) also matches everything under it, and a bare
/// word (`local_fetch`) also matches everything under a directory whose name
/// contains it, so naming a directory scopes to its files instead of
/// matching nothing. A blank entry names nothing and adds no glob (it would
/// read as `**`, which matches every path and lifts every ignore).
pub(crate) fn include_globs(patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .filter(|pattern| !pattern.trim().trim_start_matches("./").is_empty())
        .flat_map(|pattern| {
            let trimmed = pattern.trim().trim_start_matches("./");
            let glob = include_glob(pattern);
            let under = if is_bare_word(trimmed) {
                Some(format!("**/{glob}/**"))
            } else {
                let plain_path = glob.contains('/') && !glob.contains(GLOB_META);
                plain_path.then(|| format!("{}/**", glob.trim_end_matches('/')))
            };
            std::iter::once(glob).chain(under)
        })
        .collect()
}

/// Whether a glob has a `..` segment. Globs match paths under the listed or
/// searched `path`, so such a glob never matches anything there.
pub(crate) fn reaches_above(glob: &str) -> bool {
    glob.split(['/', '\\']).any(|segment| segment == "..")
}

/// The warning for the first include or exclude glob that reaches above
/// `path` (see [`reaches_above`]): why it never matches, and the repair.
pub(crate) fn parent_glob_warning(include: &[String], exclude: &[String]) -> Option<String> {
    let glob = include
        .iter()
        .chain(exclude)
        .find(|glob| reaches_above(glob))?;
    Some(format!(
        "Glob `{glob}` never matches: globs match paths under `path`, and a `..` segment reaches above it; widen `path` instead."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_entry_adds_no_glob() {
        let globs = include_globs(&["".to_owned(), "  ".to_owned(), "./".to_owned()]);
        assert!(globs.is_empty(), "{globs:?}");
        assert_eq!(
            include_globs(&["".to_owned(), "src".to_owned()]),
            ["*src*", "**/*src*/**"]
        );
    }

    #[test]
    fn a_glob_with_a_parent_segment_is_found() {
        let include = ["*.rs".to_owned()];
        let exclude = ["../src/*.ts", "a/../b"].map(str::to_owned);
        let warning = parent_glob_warning(&include, &exclude).expect("warning");
        assert!(
            warning.starts_with("Glob `../src/*.ts` never matches"),
            "{warning}"
        );
        assert!(reaches_above("a\\..\\b"));
        assert!(!reaches_above("..rc"));
        assert!(!reaches_above("src/..x/*.rs"));
        assert_eq!(parent_glob_warning(&["*.md".to_owned()], &[]), None);
    }

    #[test]
    fn a_bare_word_matches_names_containing_it_and_globs_stay() {
        assert_eq!(include_glob("test"), "*test*");
        assert_eq!(include_glob("./src/api/x.ts"), "src/api/x.ts");
        assert_eq!(include_glob("*.rs"), "*.rs");
        assert_eq!(include_glob("src/**/*.ts"), "src/**/*.ts");
        assert_eq!(include_glob("{a,b}.go"), "{a,b}.go");
    }

    #[test]
    fn a_plain_path_also_matches_everything_under_it() {
        let globs = |entries: &[&str]| {
            include_globs(
                &entries
                    .iter()
                    .map(|entry| (*entry).to_owned())
                    .collect::<Vec<_>>(),
            )
        };
        assert_eq!(
            globs(&["octocode-mcp/src"]),
            ["octocode-mcp/src", "octocode-mcp/src/**"]
        );
        assert_eq!(globs(&["src/api/"]), ["src/api/", "src/api/**"]);
        assert_eq!(
            globs(&["test", "src/**/*.ts"]),
            ["*test*", "**/*test*/**", "src/**/*.ts"]
        );
    }

    #[test]
    fn a_bare_word_also_matches_under_dirs_containing_it() {
        assert_eq!(include_globs(&["src".to_owned()]), ["*src*", "**/*src*/**"]);
        // Globs and paths keep their own reading.
        assert_eq!(include_globs(&["*.rs".to_owned()]), ["*.rs"]);
    }
}
