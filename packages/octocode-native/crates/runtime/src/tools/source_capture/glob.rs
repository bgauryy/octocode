//! A clasify resource whose localFetch `path` is a glob: one resource per
//! matching file. The glob is the one dialect of every local walk
//! ([`octocode_engine::search::glob`]), anchored at its directory: the
//! longest prefix of segments that names an existing entry (so a route
//! directory `app/[id]` is that directory). Its walk is a local search's:
//! the path policy a provider-bound read runs under (credential files
//! withheld), links never followed, dot entries and git-ignored entries
//! skipped as the text search skips them, and the default prune lifted for
//! a directory the glob names. Every matching file becomes a resource, in
//! walk order: nothing is capped. What the walk left out where the glob
//! reaches is disclosed with the `skippedPaths` entries a local search
//! gives ([`crate::tools::leads::skipped`]), one path per cause.
use super::SourceReader;
use crate::execution::ExecutionContext;
use crate::id::ToolId;
use crate::policy::discovery::{WalkAdmission, WalkRules};
use crate::tools::judgment::ClassificationError;
use crate::tools::leads::skipped::{
    SkippedEntry, WalkErrors, skipped_causes, skipped_paths, under, unlifted,
};
use octocode_contract_types::{ErrorCode, SkippedPathCause};
use octocode_engine::search::glob::{GLOB_META, IncludeScope, matches};
use octocode_engine::search::text_search::TextSearchPathFilter as _;
use serde_json::{Map, Value, json};
use std::path::Path;
use std::sync::Arc;

/// What a glob that matched no file says.
const NO_MATCH_HINT: &str = "A glob walks as local searches do: it skips dot entries, git-ignored files and default-excluded directories, and matches names case-sensitively. Write a literal [ as \\[, or name a file by its path.";

/// What a glob that does not compile says.
const INVALID_HINT: &str = "Correct the glob: * and ? stay inside one directory, ** spans directories, [...] is a class and {a,b} alternates.";

/// What the listing of a glob's skipped paths is.
const LISTING_HINT: &str = "The structureSearch call in related[] lists every file the glob matches, with those skips lifted.";

/// What a glob whose directory lies outside the allowed roots says.
const OUTSIDE_ROOTS_HINT: &str = "Start the glob at a directory inside the allowed roots.";

/// A `resources` map with every glob expanded, in input order and, within a
/// glob, in walk order; and what each glob's walk left out.
pub(crate) struct Expanded {
    pub resources: Vec<(String, Result<Value, ClassificationError>)>,
    pub notices: Vec<String>,
    /// Ready calls that list what each glob's walk left out.
    pub related: Vec<Value>,
}

/// Expand every localFetch resource of `resources` whose path is a glob into
/// `<key>/<path under the glob's directory>`, each with the rest of its query.
/// A glob that matches nothing, cannot be walked, or whose walk the call
/// stopped, is its resource's error.
pub(crate) fn expand(
    resources: &Map<String, Value>,
    reader: &dyn SourceReader,
    context: &ExecutionContext,
) -> Expanded {
    let mut expanded = Expanded {
        resources: Vec::new(),
        notices: Vec::new(),
        related: Vec::new(),
    };
    // A caller's own key is never taken by an expanded one.
    let mut taken = resources
        .keys()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    for (key, source) in resources {
        let Some(pattern) = glob_path(source, reader) else {
            expanded.resources.push((key.clone(), Ok(source.clone())));
            continue;
        };
        let Matched {
            files,
            skipped,
            listing,
        } = match matching(pattern, reader, context) {
            Ok(found) => found,
            Err(error) => {
                expanded.resources.push((key.clone(), Err(error)));
                continue;
            }
        };
        if files.is_empty() {
            let mut error = ClassificationError::new(
                ErrorCode::NotFound,
                format!("No file matches the glob '{pattern}'; nothing was judged."),
                NO_MATCH_HINT,
            );
            error.hints.extend(skipped);
            if let Some(listing) = listing {
                error.hints.push(LISTING_HINT.to_owned());
                error.leads.push(("includeSkipped".to_owned(), listing));
            }
            expanded.resources.push((key.clone(), Err(error)));
            continue;
        }
        for notice in skipped
            .into_iter()
            .chain(listing.is_some().then(|| LISTING_HINT.to_owned()))
        {
            if !expanded.notices.contains(&notice) {
                expanded.notices.push(notice);
            }
        }
        if let Some(listing) = listing
            && !expanded.related.contains(&listing)
        {
            expanded.related.push(listing);
        }
        for (relative, path) in files {
            let mut each = source.clone();
            each["query"]["path"] = json!(path);
            let each_key = free_key(&format!("{key}/{relative}"), |key| taken.contains(key));
            taken.insert(each_key.clone());
            expanded.resources.push((each_key, Ok(each)));
        }
    }
    expanded
}

/// What one glob's walk found: each file as its path under the glob's
/// directory and as the caller spells it, the hint naming what it left out,
/// and the listing of all of that.
#[derive(Default)]
struct Matched {
    files: Vec<(String, String)>,
    skipped: Option<String>,
    listing: Option<Value>,
}

/// `wanted`, or the first free `<wanted>-<n>` (n from 2) when `taken` holds
/// it: the one rule for a key two sources would share.
pub(crate) fn free_key(wanted: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut key = wanted.to_owned();
    let mut n = 1;
    while taken(&key) {
        n += 1;
        key = format!("{wanted}-{n}");
    }
    key
}

/// The glob `source` names: a localFetch path with a glob character that
/// names no existing entry (a file named `[id].tsx` is that file).
fn glob_path<'a>(source: &'a Value, reader: &dyn SourceReader) -> Option<&'a str> {
    let path = source["query"]["path"].as_str()?;
    (source["tool"] == ToolId::LocalFetch.as_str()
        && path.contains(GLOB_META)
        && !exists(reader, path))
    .then_some(path)
}

/// Whether `path` names an existing entry, resolved as the policy resolves it.
fn exists(reader: &dyn SourceReader, path: &str) -> bool {
    std::fs::symlink_metadata(reader.paths().expand_and_resolve(Path::new(path))).is_ok()
}

/// Every file `pattern` matches, in walk order, as its path under the glob's
/// directory and as the caller spells it; the disclosure of what the walk
/// left out where the glob reaches; and the listing that names all of it.
fn matching(
    pattern: &str,
    reader: &dyn SourceReader,
    context: &ExecutionContext,
) -> Result<Matched, ClassificationError> {
    let segments = pattern.split('/').collect::<Vec<_>>();
    // The directory is every leading segment that is literal: one with no
    // glob character, or one that names an existing entry as spelled.
    let mut at = 0;
    while at < segments.len()
        && (!segments[at].contains(GLOB_META)
            || (at + 1 < segments.len() && exists(reader, &segments[..=at].join("/"))))
    {
        at += 1;
    }
    if at == segments.len() {
        return Ok(Matched::default());
    }
    let base = match segments[..at].join("/") {
        base if base.is_empty() && at > 0 => "/".to_owned(),
        base => base,
    };
    let anchored = format!("/{}", segments[at..].join("/"));
    let root = reader
        .paths()
        .validate(if base.is_empty() { "." } else { &base })
        .map_err(|error| unwalkable(pattern, &base, &error, reader))?
        .canonical;
    let invalid = |error: &dyn std::fmt::Display| {
        ClassificationError::new(
            ErrorCode::InvalidGlob,
            format!("The glob '{pattern}' does not compile: {error}."),
            INVALID_HINT,
        )
    };
    let Some(matcher) =
        octocode_engine::search::glob::set(&root, std::iter::once(anchored.as_str()), "path")
            .map_err(|error| invalid(&error))?
    else {
        return Ok(Matched::default());
    };
    let include = [anchored];
    let admission = std::sync::Arc::new(WalkAdmission::new(
        crate::policy::path::DiscoveryWalk::owned(reader.paths().clone()),
        &root,
        WalkRules {
            defaults: true,
            include: &include,
            generated: false,
            engine_prunes: false,
        },
    ));
    // Every entry reaches the filter, which applies the ignore and dot-entry
    // rules itself, as the text search does, so what they leave out is
    // recorded instead of vanishing inside the walker.
    let mut walk = octocode_engine::search::walk::walk_builder(
        &root,
        octocode_engine::types::IgnorePolicy::ALL,
        None,
    );
    walk.sort_by_file_name(std::cmp::Ord::cmp);
    let rules = Arc::new(octocode_engine::search::walk::IgnoreRules::with_global(
        &root,
        reader.paths().global_ignore(),
    ));
    {
        let (admission, matcher, root) = (admission.clone(), matcher.clone(), root.clone());
        let scope = IncludeScope::new(&root, &include);
        let rules = Arc::clone(&rules);
        walk.filter_entry(move |entry| {
            let path = entry.path();
            let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
            if entry.depth() > 0 {
                if rules.is_ignored(path, is_dir) {
                    admission.skipped_ignored(path, is_dir);
                    return false;
                }
                if entry.file_name().as_encoded_bytes().first() == Some(&b'.') {
                    admission.skipped_hidden(path, is_dir);
                    return false;
                }
            }
            // A withheld entry is disclosed when the glob could reach it.
            admission.admit(path, entry.file_type(), |is_dir| {
                if is_dir {
                    scope.reaches(path, path.strip_prefix(&root).unwrap_or(path), true)
                } else {
                    matches(&matcher, path, false)
                }
            })
        });
    }
    let spelled = |relative: &str| spell(&base, relative);
    let mut files = Vec::new();
    let mut errors = WalkErrors::default();
    for entry in walk.build() {
        context.check().map_err(super::page::stopped)?;
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                errors.skipped += 1;
                if error
                    .io_error()
                    .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
                {
                    errors.denied += 1;
                }
                continue;
            }
        };
        if entry.file_type().is_some_and(|kind| kind.is_file())
            && matches(&matcher, entry.path(), false)
        {
            let relative = octocode_engine::text::slash_path(
                entry.path().strip_prefix(&root).unwrap_or(entry.path()),
            );
            files.push((relative.clone(), spelled(&relative)));
        }
    }
    // An ignore file the walk could not read applied no rules: one more
    // unreadable entry.
    for file in rules.unreadable_rule_files() {
        errors.skipped += 1;
        if file.kind == std::io::ErrorKind::PermissionDenied {
            errors.denied += 1;
        }
    }
    let skips = admission.skipped();
    let left = under(&root, SkippedPathCause::Ignored, &skips.ignored)
        .chain(under(
            &root,
            SkippedPathCause::DefaultExcluded,
            &skips.pruned,
        ))
        .chain(under(
            &root,
            SkippedPathCause::DefaultExcluded,
            &skips.generated,
        ))
        .chain(under(&root, SkippedPathCause::Hidden, &skips.hidden))
        .chain(under(
            &root,
            SkippedPathCause::OctocodeArtifact,
            &skips.artifacts,
        ))
        .chain(unlifted(&root.to_string_lossy(), &skips.withheld, errors));
    let disclosed = disclose_skips(pattern, &root, &base, left.collect(), &skips.counts());
    // The skipped paths a flag lifts, listed whole: the glob's own walk with
    // exactly those causes lifted, when the contract admits the call.
    let causes = skipped_causes(&skips);
    let listing = (!causes.is_empty())
        .then(|| {
            let row = json!({"operation":"files","path":if base.is_empty() { "." } else { base.as_str() },
                "include":include,"entryType":"file"});
            let mut lifted = row.clone();
            crate::tools::empty_walk::lift(&mut lifted, &causes);
            crate::contracts::validate_built_query(ToolId::StructureSearch.as_str(), lifted)
                .ok()
                .map(|_| crate::tools::leads::skipped::lead(ToolId::StructureSearch, &row, &causes))
        })
        .flatten();
    Ok(Matched {
        files,
        skipped: disclosed,
        listing,
    })
}

/// The hint naming what a glob's walk left out (`entries`, under `root`, the
/// glob's directory as the caller spells it, `base`).
pub(super) fn disclose_skips(
    pattern: &str,
    root: &Path,
    base: &str,
    entries: Vec<SkippedEntry>,
    counts: &str,
) -> Option<String> {
    // A path the contract cannot carry costs only its own name.
    let (named, lost): (Vec<_>, Vec<_>) = entries
        .into_iter()
        .partition(|entry| skipped_paths(std::iter::once(entry.clone()), false, false).is_ok());
    let mut shown = match skipped_paths(named, false, false) {
        Ok(left) => left
            .map(|left| {
                left.entries
                    .iter()
                    .map(|entry| {
                        let path = Path::new(&**entry.path);
                        let inside = path.strip_prefix(root).unwrap_or(path);
                        crate::tools::leads::skipped::disclosure(
                            entry,
                            &spell(base, &octocode_engine::text::slash_path(inside)),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
        Err(_) => vec![counts.to_owned()],
    };
    let lost = lost.iter().map(|entry| entry.count).sum::<usize>();
    if lost > 0 {
        shown.push(format!("{lost} more that cannot be named"));
    }
    (!shown.is_empty()).then(|| skipped_hint(pattern, &shown.join(", ")))
}

/// Why the glob's directory `base` cannot be walked, with the recovery of
/// that cause: outside the allowed roots, the roots; missing, that the
/// directory does not exist and the nearest one that does; any other, the
/// recovery a localFetch read of it gives.
fn unwalkable(
    pattern: &str,
    base: &str,
    error: &crate::policy::PolicyError,
    reader: &dyn SourceReader,
) -> ClassificationError {
    let message = format!("The glob '{pattern}' cannot be walked: {error}");
    let dir = if base.is_empty() { "." } else { base };
    let hints = match error.code {
        crate::policy::PolicyErrorCode::OutsideAllowedRoots => {
            return ClassificationError::new(error.shared_code(), message, OUTSIDE_ROOTS_HINT);
        }
        crate::policy::PolicyErrorCode::NotFound => vec![missing_directory_hint(dir)],
        _ => Vec::new(),
    };
    super::resource::local_recovery(
        ClassificationError::with_hints(error.shared_code(), message, hints),
        &json!({ "path": dir }),
        reader,
    )
}

/// What a glob whose directory `dir` does not exist says.
fn missing_directory_hint(dir: &str) -> String {
    format!("The glob's directory `{dir}` does not exist; start the glob at one that does.")
}

/// A path under the glob's directory as the caller spells it (`base`).
fn spell(base: &str, relative: &str) -> String {
    match (base, relative) {
        ("", "") => ".".to_owned(),
        ("", relative) => relative.to_owned(),
        (base, "") => base.to_owned(),
        (base, relative) => format!("{}/{relative}", base.trim_end_matches('/')),
    }
}

/// What a glob's walk left out where the glob reaches, one path per cause.
fn skipped_hint(pattern: &str, shown: &str) -> String {
    format!(
        "The glob '{pattern}' left out paths it could match: {shown}. A hidden, ignored or default-excluded file is judged when named by its path."
    )
}
