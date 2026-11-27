//! ghSearchCode query syntax.
use octocode_contract_types::GhSearchCodeQuery;
use octocode_github::{qualifier_value, search_phrase};
use octocode_tools_shared::gh_shared::query::SearchTerms;

/// What `path` names in the search. GitHub's `path:` qualifier matches a
/// directory prefix and never a file's own path; `filename:` given the full
/// path matches files whose path ends with it, so a file is that qualifier
/// under its parent directory (`path:/` for the root).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PathAs {
    Folder,
    File,
}

impl PathAs {
    /// Whether a hit is the file a `File` scope names: `filename:` matches a
    /// path suffix, so a deeper path that ends the same way is another file.
    /// GitHub matches paths without case, so this does too.
    pub(super) fn keeps(self, query: &GhSearchCodeQuery, hit: &str) -> bool {
        match (self, query.path.as_deref()) {
            (Self::File, Some(path)) => {
                hit.trim_matches('/').to_lowercase() == path.trim_matches('/').to_lowercase()
            }
            _ => true,
        }
    }
}

/// The search query; `extension` narrows a multi-extension query to one of
/// them (GitHub ANDs repeated `extension:` qualifiers); `path_as` says what
/// `path` names.
pub(super) fn code(query: &GhSearchCodeQuery, extension: Option<&str>, path_as: PathAs) -> String {
    let GhSearchCodeQuery {
        keywords,
        owner,
        repo,
        path,
        extensions,
        name_contains,
        match_in,
        ..
    } = query;
    let mut terms = SearchTerms::new(keywords.iter().map(|keyword| keyword.as_str()));
    // A folder `path` is a directory prefix; a dotted segment such as
    // `types/lodash.merge` stays one, never split into a `filename:`.
    terms.text(
        "filename",
        name_contains.as_ref().map(|value| value.as_str()),
    );
    match extension {
        Some(extension) => terms.text("extension", Some(extension)),
        None => {
            for extension in extensions {
                terms.text("extension", Some(extension.as_str()));
            }
        }
    }
    if let Some(path) = path.as_deref() {
        match path_as {
            PathAs::Folder => push_path(&mut terms, "path", path),
            PathAs::File => {
                let file = path.trim_matches('/');
                push_path(&mut terms, "filename", file);
                match file.rsplit_once('/') {
                    Some((parent, _)) => push_path(&mut terms, "path", parent),
                    None => terms.push("path:/".to_owned()),
                }
            }
        }
    }
    terms.scope(Some(owner.as_str()), repo.as_deref().map(String::as_str));
    let locations = match_in
        .iter()
        .map(|location| match location {
            octocode_contract_types::GhSearchCodeQueryMatchInItem::Content => "file",
            octocode_contract_types::GhSearchCodeQueryMatchInItem::Path => "path",
        })
        .collect::<Vec<_>>()
        .join(",");
    terms.text("in", Some(&locations));
    terms.finish()
}

/// A `qualifier:` holding a repository path, quoted when it has a separator.
fn push_path(terms: &mut SearchTerms, qualifier: &str, path: &str) {
    let value = if path.contains('/') || path.contains('@') {
        search_phrase(path)
    } else {
        qualifier_value(path)
    };
    if !value.is_empty() {
        terms.push(format!("{qualifier}:{value}"));
    }
}

#[cfg(test)]
mod tests;
