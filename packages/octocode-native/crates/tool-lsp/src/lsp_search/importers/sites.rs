//! What the name on given lines of answered files binds, asked of the
//! server: a spelling the answer does not list is a use it left out only
//! when it names the anchor's declaration.
use super::*;

/// What `symbol` binds on each of `files`' zero-based lines (in the order
/// given), by the definition chain of every occurrence on the line, judged
/// against the anchor's declaration ([`binding`]). A line binds the anchor
/// when an occurrence on it does; it binds another declaration when every
/// occurrence the server placed does; an occurrence the server names no
/// declaration for (a comment after code, code it does not analyze) says
/// nothing, so a line of only those stays undecided, as does every line of
/// a file that could not be read or opened. `None` when the anchor's own
/// declaration did not resolve: nothing can be compared with it.
pub(in super::super) async fn line_bindings(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    anchor: Position<'_>,
    symbol: &str,
    files: &[(String, Vec<u32>)],
) -> Result<Option<Vec<Vec<Binding>>>, LspFailure> {
    let declaration = identities(
        client,
        sources,
        snippet_policy,
        cancel,
        anchor.path,
        anchor.line,
        anchor.character,
    )
    .await?
    .filter(|found| !found.is_empty());
    let Some(declaration) = declaration else {
        return Ok(None);
    };
    let names = files
        .iter()
        .map(|(file, _)| file.clone())
        .collect::<Vec<_>>();
    let opened = open_candidates(client, sources, cancel, anchor.path, &names, &|_| {
        vec![symbol.to_owned()]
    })
    .await?;
    let mut opened = opened.files.into_iter().collect::<HashMap<_, _>>();
    let policy = sources.policy();
    let missing = proven_missing(policy);
    let mut bindings = Vec::with_capacity(files.len());
    for (file, lines) in files {
        let Some(spots) = opened.remove(file) else {
            bindings.push(vec![Binding::Undecided; lines.len()]);
            continue;
        };
        let content = match sources.get(file).await {
            Some(source) => source.content.clone(),
            None => String::new(),
        };
        // What each of the file's own bindings was proven to name (see
        // [`verify_file`]): its other sites are decided without a request.
        let mut settled = HashMap::<u32, Binding>::new();
        let mut verdicts = Vec::with_capacity(lines.len());
        for line in lines {
            let mut verdict = None;
            for spot in spots.iter().filter(|spot| spot.line == *line) {
                cancel.check().map_err(LspFailure::cancelled)?;
                let said = match spot.binding.and_then(|id| settled.get(&id).copied()) {
                    Some(known) => Some(known),
                    None => {
                        let resolved = identities(
                            client,
                            sources,
                            snippet_policy,
                            cancel,
                            file,
                            spot.line,
                            spot.character,
                        )
                        .await?
                        .filter(|resolved| !resolved.is_empty());
                        resolved.map(|resolved| {
                            let said = binding(&resolved, file, &declaration, &content, &missing);
                            if let Some(id) = spot.binding
                                && said != Binding::Undecided
                            {
                                settled.insert(id, said);
                            }
                            said
                        })
                    }
                };
                verdict = match (verdict, said) {
                    (Some(Binding::Anchor), _) | (_, Some(Binding::Anchor)) => {
                        Some(Binding::Anchor)
                    }
                    (Some(Binding::Undecided), _) | (_, Some(Binding::Undecided)) => {
                        Some(Binding::Undecided)
                    }
                    (kept, said) => kept.or(said),
                };
                if verdict == Some(Binding::Anchor) {
                    break;
                }
            }
            verdicts.push(verdict.unwrap_or(Binding::Undecided));
        }
        bindings.push(verdicts);
    }
    Ok(Some(bindings))
}
