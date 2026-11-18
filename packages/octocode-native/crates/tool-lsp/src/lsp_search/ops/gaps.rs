//! The identity of the code sites an answer does not list inside the files
//! it covers: the server says which of them name the anchor's declaration.
use super::*;

/// Ask the server what `name` binds at each site the settled answer does
/// not list in a file it covers ([`Scope::check_answer_gaps`]): a site
/// that names the anchor's declaration (`anchor`) is a use the answer left
/// out, one that names another declaration is no gap, and one the server
/// cannot place stays unverified. Run once the answer stands (an answer
/// asked again during a load recorded more lines) and before its gaps are
/// flagged. A check that could not run (the anchor's declaration
/// unresolved, a request failed) leaves every candidate unverified; only
/// cancellation ends the request.
pub(in super::super) async fn verify_answer_gaps(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    scope: &Scope,
    anchor: Position<'_>,
    name: &str,
) -> Result<(), LspFailure> {
    // What an earlier ask of this request left is not this answer's.
    scope.forget_gaps();
    let candidates = scope.check_answer_gaps(name, cancel).await?;
    if candidates.is_empty() {
        return Ok(());
    }
    let files = candidates
        .iter()
        .map(|gap| (gap.file.clone(), gap.unverified.clone()))
        .collect::<Vec<_>>();
    let bindings = match importers::line_bindings(
        client,
        sources,
        snippet_policy,
        cancel,
        anchor,
        name,
        &files,
    )
    .await
    {
        Ok(Some(bindings)) => bindings,
        Ok(None) => return Ok(()),
        Err(failure) if failure.is_cancelled() => return Err(failure),
        Err(_) => return Ok(()),
    };
    let settled = files
        .into_iter()
        .zip(bindings)
        .map(|((file, lines), verdicts)| {
            let of = |wanted: importers::Binding| {
                lines
                    .iter()
                    .zip(&verdicts)
                    .filter(|(_, verdict)| **verdict == wanted)
                    .map(|(line, _)| *line)
                    .collect::<Vec<_>>()
            };
            super::super::scope::Gap {
                left_out: of(importers::Binding::Anchor),
                unverified: of(importers::Binding::Undecided),
                file,
            }
        })
        .collect();
    scope.settle_gaps(name, settled);
    Ok(())
}
