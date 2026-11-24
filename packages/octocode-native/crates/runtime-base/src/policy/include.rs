//! Scope diagnostics for authored include and exclude globs.

/// Whether a glob has a `..` segment. Globs match paths under the listed or
/// searched `path`, so such a glob never matches anything there.
pub fn reaches_above(glob: &str) -> bool {
    glob.split(['/', '\\']).any(|segment| segment == "..")
}

/// The warning for the first include or exclude glob that reaches above
/// `path` (see [`reaches_above`]): why it never matches, and the repair.
pub fn parent_glob_warning(include: &[String], exclude: &[String]) -> Option<String> {
    let glob = include
        .iter()
        .chain(exclude)
        .find(|glob| reaches_above(glob))?;
    Some(format!(
        "Glob `{glob}` never matches: globs match paths under `path`, and a `..` segment reaches above it; widen `path` instead."
    ))
}

/// The empty result's fact when include/exclude left the walk no file: one
/// text for every tool that walks a scope (localSearch, astSearch).
pub const GLOB_MISS_HINT: &str = "Nothing searched: include/exclude matched no file. A glob with `/` matches from `path`, not the workspace root, so `src/*.rs` is `path`/src/*.rs: start it with `**/` to match at any depth. The related[] structureSearch request lists the names to fix the globs.";

/// The empty result's fact when the only files include/exclude matched are
/// `withheld` ones the security path policy keeps from every walk: the
/// globs did match, so [`GLOB_MISS_HINT`]'s advice to fix them would send
/// the caller respelling a glob that already works. It states the count and
/// the cause, and names no file: a withheld name is disclosed only where the
/// path policy itself discloses it. With nothing withheld it is
/// [`GLOB_MISS_HINT`] followed by each glob as it was read
/// ([`globs_as_read`]).
pub fn glob_miss_hint(
    withheld: usize,
    include: &[String],
    exclude: &[String],
) -> std::borrow::Cow<'static, str> {
    if withheld == 0 {
        return match globs_as_read(include, exclude) {
            Some(read) => format!("{GLOB_MISS_HINT} {read}").into(),
            None => GLOB_MISS_HINT.into(),
        };
    }
    let (files, them) = if withheld == 1 {
        ("1 withheld file".to_owned(), "it")
    } else {
        (format!("{withheld} withheld files"), "them")
    };
    format!(
        "Nothing searched: include/exclude matched {files} and no other file. The security path policy withholds {them} (credential-named files and sensitive directories), and no query field lifts that: do not respell the globs to reach {them}."
    )
    .into()
}

/// The globs of a request as they were read, each list with its count: a
/// caller who meant several globs and sent one string sees one glob.
fn globs_as_read(include: &[String], exclude: &[String]) -> Option<String> {
    let listed = |name: &str, globs: &[String]| {
        (!globs.is_empty()).then(|| {
            let each = globs
                .iter()
                .map(|glob| format!("`{glob}`"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} {name} ({each})", globs.len())
        })
    };
    let lists = [listed("include", include), listed("exclude", exclude)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    (!lists.is_empty()).then(|| format!("Globs as read: {}.", lists.join(", ")))
}

#[cfg(test)]
mod tests;
