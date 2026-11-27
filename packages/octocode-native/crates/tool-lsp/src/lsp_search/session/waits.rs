//! Session warnings and failures of a language server the call leased.

use super::{Exit, LspFailure, LspSearchQuery, Session, Target, crashed};

/// The warning for a pool that holds more resident memory than its budget
/// after this call's acquire (`resident`, `budget` in bytes): a server busy
/// with another request keeps it over, and the pool stops that server as its
/// request ends.
pub(in crate::lsp_search) fn disclose_pool_overflow(resident: u64, budget: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    format!(
        "The language server pool holds about {} MiB, over its {} MiB budget, while other servers finish their requests; the pool stops each as its request ends.",
        resident.div_ceil(MIB),
        budget.div_ceil(MIB)
    )
}

/// [`crashed`] for a failure of an operation `session` served: a timeout
/// names what the server last said is wrong with it.
pub(in crate::lsp_search) fn session_failed(
    query: &LspSearchQuery,
    target: &Target,
    session: &Session,
    failure: LspFailure,
) -> Exit {
    let failure = failure.with_server_health(session.client.health());
    crashed(query, &target.uri, &session.config, failure)
}

#[cfg(test)]
mod tests;
