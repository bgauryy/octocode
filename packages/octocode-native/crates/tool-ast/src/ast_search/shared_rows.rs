//! Identical `symbols` rows asked at once, such as the rows of one batch,
//! share one walk, outline, page and judging of their sources
//! ([`octocode_tools_shared::page_memo::Shared`]). The first row leads; each
//! row that asked for the same page while it was built takes the page it
//! built, unjudged, while what proves it holds
//! ([`super::symbols::execute_symbols_proven`]: the root, every listed file,
//! every admitted directory with its ignore files, the ignore rules outside
//! the walk and the repository markers, unchanged since before the leader's
//! walk), and judges it for its own request from the sources the leader
//! judged ([`Known`]). A row whose check fails, or that waited past its own
//! stop, computes its page itself. Nothing is kept once the leader is done.
use super::judged_rows::{Known, NamePlaces, drop_judging_positions, judge_rows_with};
use octocode_contract_types::AstSearchQuerySymbols;
use octocode_runtime_base::response::result::ToolError;
use octocode_tools_shared::page_memo::Shared;
use serde_json::{Value, json};

/// The pages being built now, by [`key`].
static ROWS: Shared<SharedRow> = Shared::new();

/// What a leading row hands the identical rows that waited on it.
struct SharedRow {
    /// Its page before judging.
    page: Value,
    places: NamePlaces,
    /// The sources it judged the page by.
    known: Known,
    proof: octocode_engine::stamp::Proof,
}

/// Every input of a page: the query, the path policy and the row's share
/// of the response window.
fn key(
    q: &AstSearchQuerySymbols,
    paths: &octocode_runtime_base::policy::path::PathPolicy,
    response_window: Option<usize>,
) -> String {
    octocode_runtime_base::digest::json_sha256(&json!([
        "symbols",
        paths.identity(),
        q,
        response_window,
    ]))
}

/// One `symbols` page, judged for `security`'s request: shared with the
/// identical rows asked at once (see the module).
pub(super) fn symbols_page(
    q: &AstSearchQuerySymbols,
    paths: &octocode_runtime_base::policy::path::PathPolicy,
    security: &octocode_runtime_base::security::ContentSecurity,
    cancel: &dyn octocode_runtime_base::execution::cancel::CancellationCheck,
    response_window: Option<usize>,
) -> Result<Value, ToolError> {
    let judge = |mut page: Value, places: NamePlaces, known: &Known| {
        let sources = octocode_tools_shared::judged_rows::RowSources::as_shown(paths, security);
        judge_rows_with(&mut page, &sources, places, known);
        drop_judging_positions(&mut page);
        page
    };
    let stop = || cancel.check().is_err();
    let key = key(q, paths, response_window);
    #[cfg(test)]
    let hooked = key.clone();
    ROWS.share(
        key,
        &stop,
        |waited| {
            #[cfg(test)]
            tests::on_lead(super::symbols::walk_path(q), &|| ROWS.waiting(&hooked));
            // A row no other row waits on proves nothing and copies nothing.
            match super::symbols::execute_symbols_proven(q, paths, cancel, response_window, waited)
            {
                Ok((page, places, proof)) => {
                    let known = Known::default();
                    let handed = proof.filter(|_| waited()).map(|proof| SharedRow {
                        page: page.clone(),
                        places: places.clone(),
                        known: known.clone(),
                        proof,
                    });
                    (Ok(judge(page, places, &known)), handed)
                }
                Err(error) => (Err(error), None),
            }
        },
        |row| {
            row.proof
                .holds()
                .then(|| Ok(judge(row.page.clone(), row.places.clone(), &row.known)))
        },
    )
}

#[cfg(test)]
mod tests;
