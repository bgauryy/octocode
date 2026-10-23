mod errors;
#[cfg(test)]
mod delivered_page_tests;
#[cfg(test)]
mod execute_tests;
#[cfg(test)]
mod fidelity_tests;
#[cfg(test)]
mod file_language_tests;
mod language;
#[cfg(test)]
mod lead_tests;
#[cfg(test)]
mod match_tests;
mod matches;
mod memo;
#[cfg(test)]
mod outline_tests;
mod output;
#[cfg(test)]
mod paging_tests;
#[cfg(test)]
mod policy_tests;
mod region;
#[cfg(test)]
mod region_tests;
#[cfg(test)]
mod rule_tests;
mod scan_window;
mod source;
mod symbols;
#[cfg(test)]
mod symbols_tests;
mod syntax;
#[cfg(test)]
mod test_rows;
mod unwalked;
#[cfg(test)]
mod unwalked_tests;

use octocode_contract_types::ErrorCode;
use serde_json::Value;

use matches::MatchQuery;
pub use octocode_contract_types::AstSearchQuery;

use crate::response::result::ToolError;
use crate::tools::display_name;

/// Row path for a descendant of a directory scope: `{root_name}/{relative}`.
/// The runtime attaches `base = parent(scope)`, so `base + path` resolves to
/// the real file for every operation (match, symbols).
pub(super) fn rooted_display(root: &std::path::Path, relative: &std::path::Path) -> String {
    let root_name = display_name(root);
    if relative.as_os_str().is_empty() {
        root_name
    } else {
        format!("{root_name}/{}", relative.to_string_lossy())
    }
}

/// The row paths of one scope as the response prints them. A row names its
/// file from the scope's parent ([`rooted_display`]), or by the file's own
/// name for a file scope; the response stage respells it workspace-relative,
/// else absolute. Pages cut from row sizes measure that delivered spelling:
/// under a deep scope it is longer than the shown one, and a page sized by
/// the shown one overflows the window it was cut for.
#[derive(Clone, Copy)]
pub(super) struct RowPaths<'a> {
    paths: &'a crate::policy::path::PathPolicy,
    scope: &'a std::path::Path,
    is_file: bool,
}

impl<'a> RowPaths<'a> {
    pub(super) fn new(
        paths: &'a crate::policy::path::PathPolicy,
        scope: &'a std::path::Path,
        is_file: bool,
    ) -> Self {
        Self {
            paths,
            scope,
            is_file,
        }
    }

    /// `shown` as the response prints it. Warning text is not rewritten by
    /// the response stage, so a path it names is spelled here, the same way,
    /// to be passed on verbatim.
    pub(super) fn delivered(&self, shown: &str) -> String {
        let file = if self.is_file {
            self.scope.to_path_buf()
        } else {
            self.scope.parent().unwrap_or(self.scope).join(shown)
        };
        self.paths
            .workspace_relative(&file)
            .unwrap_or_else(|| file.to_string_lossy().into_owned())
    }

    /// The characters `shown` takes in a row as delivered: its JSON string
    /// body in UTF-16 units, as the response window counts them.
    pub(super) fn chars(&self, shown: &str) -> usize {
        octocode_engine::json_size::string_body_units(&self.delivered(shown))
    }
}

pub type AstResult = Result<Value, ToolError>;

/// Execute one typed row. The runtime parses the validated row with its
/// shared `parse_query`, so a shape mismatch has one code across tools.
pub fn execute_ast(
    query: &AstSearchQuery,
    paths: &crate::policy::path::PathPolicy,
    security: &crate::security::ContentSecurity,
    cancellation: &dyn crate::execution::cancel::CancellationCheck,
    response_window: Option<usize>,
) -> AstResult {
    let requested = match query {
        AstSearchQuery::Symbols(query) => query.path.as_str(),
        AstSearchQuery::MatchPattern(query) => query.path.as_str(),
        AstSearchQuery::MatchRule(query) => query.path.as_str(),
        AstSearchQuery::SyntaxTree(query) => query.path.as_str(),
    };
    run_ast(query, paths, security, cancellation, response_window)
        .map_err(|error| missing_path_recovery(error, requested, paths))
}

/// A `pathNotFound` for the query's path leads to the tree of its nearest
/// existing parent ([`missing_path_lead`]), the recovery every local tool
/// gives a missing path; its message names no absolute filesystem path.
///
/// [`missing_path_lead`]: crate::response::result::missing_path_lead
fn missing_path_recovery(
    mut error: ToolError,
    requested: &str,
    paths: &crate::policy::path::PathPolicy,
) -> ToolError {
    if error.code != ErrorCode::PathNotFound || error.next.is_some() {
        return error;
    }
    error.next = crate::response::result::missing_path_lead(paths, requested).map(Box::new);
    error
}

fn run_ast(
    query: &AstSearchQuery,
    paths: &crate::policy::path::PathPolicy,
    security: &crate::security::ContentSecurity,
    cancellation: &dyn crate::execution::cancel::CancellationCheck,
    response_window: Option<usize>,
) -> AstResult {
    match query {
        AstSearchQuery::Symbols(query) => {
            execute_symbols(query, paths, cancellation, response_window)
        }
        AstSearchQuery::MatchPattern(query) => execute_match(
            MatchQuery::Pattern(query),
            paths,
            cancellation,
            response_window,
        ),
        AstSearchQuery::MatchRule(query) => execute_match(
            MatchQuery::Rule(query),
            paths,
            cancellation,
            response_window,
        ),
        AstSearchQuery::SyntaxTree(query) => {
            execute_syntax(query, paths, security, cancellation, response_window)
        }
    }
}

pub use matches::execute_match;
pub(crate) use output::Output;
pub use symbols::execute_symbols;
pub use syntax::execute_syntax;

/// A dot-named entry (`.git`, `.env`): out of view of a walk without
/// `hidden`.
pub(super) fn is_dot_entry(path: &std::path::Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
}
