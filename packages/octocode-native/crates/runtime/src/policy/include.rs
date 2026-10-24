//! Scope diagnostics for authored include and exclude globs.

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
}
