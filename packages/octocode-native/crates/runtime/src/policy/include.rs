//! The one reading of an `include` entry every tool shares: a glob as
//! written, or a bare word (no glob metacharacter) that matches names
//! containing it and everything under a directory whose name contains it.

/// The glob an `include` entry stands for: a bare word (no `/`, no glob
/// metacharacter) becomes `*word*`, names containing it at any depth; any
/// other entry is a glob as written, with a leading `./` dropped.
pub fn include_glob(pattern: &str) -> String {
    let pattern = pattern.trim().trim_start_matches("./");
    if pattern.contains(['/', '*', '?', '[', '{']) {
        pattern.to_owned()
    } else {
        format!("*{pattern}*")
    }
}

/// Every `include` entry as a glob. A path without glob metacharacters
/// (`src/api`, `src/api/x.ts`) also matches everything under it, and a bare
/// word (`local_fetch`) also matches everything under a directory whose name
/// contains it, so naming a directory scopes to its files instead of
/// matching nothing.
pub fn include_globs(patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .flat_map(|pattern| {
            let trimmed = pattern.trim().trim_start_matches("./");
            let glob = include_glob(pattern);
            let under = if !trimmed.contains(['/', '*', '?', '[', '{']) {
                Some(format!("**/{glob}/**"))
            } else {
                let plain_path = glob.contains('/') && !glob.contains(['*', '?', '[', '{']);
                plain_path.then(|| format!("{}/**", glob.trim_end_matches('/')))
            };
            std::iter::once(glob).chain(under)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
