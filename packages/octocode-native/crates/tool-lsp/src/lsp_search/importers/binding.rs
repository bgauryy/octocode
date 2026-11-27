//! What a candidate site binds: its definition chain's identities, judged
//! against the anchor's declaration.
use super::*;

/// What the name at one candidate site binds, by its definition chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum Binding {
    /// The anchor's declaration alone: a reference.
    Anchor,
    /// Another declaration: not a reference.
    Other,
    /// Neither proven: no reference, and the file stays unverified.
    Undecided,
}

/// What the name at a site of `file` (canonical) binds, from the `resolved`
/// identities of its definition chain. It binds the anchor only when every
/// identity is the anchor's `declaration` or lies in a file that declares
/// it (the module's own export of it). A site that also binds a
/// declaration of `file` itself (a same-named `const`, parameter or
/// destructured binding the site's scope declares, which shadows the
/// anchor; a shorthand `{ name }` names it and, through a contextual type,
/// the anchor's property too) binds that declaration. Any other extra
/// identity, an import binding of `file` the server did not follow
/// included, leaves the site undecided. A chain that misses the anchor is
/// another declaration unless it stopped on such an import.
pub(in super::super) fn binding(
    resolved: &HashSet<String>,
    file: &str,
    declaration: &HashSet<String>,
    content: &str,
    missing: &dyn Fn(&Path) -> bool,
) -> Binding {
    if resolved.is_disjoint(declaration) {
        return if stays_on_import(resolved, file, declaration, content, missing) {
            Binding::Undecided
        } else {
            Binding::Other
        };
    }
    let declaring = declaration
        .iter()
        .map(|identity| identity_file(identity))
        .collect::<HashSet<_>>();
    let extra = resolved
        .difference(declaration)
        .filter(|identity| !declaring.contains(identity_file(identity)))
        .collect::<Vec<_>>();
    if extra.is_empty() {
        return Binding::Anchor;
    }
    let own = extra
        .into_iter()
        .filter(|identity| identity_file(identity) == file)
        .cloned()
        .collect::<HashSet<_>>();
    if own.is_empty() || stays_on_import(&own, file, declaration, content, missing) {
        Binding::Undecided
    } else {
        Binding::Other
    }
}

/// The file of a `path:line:character:endLine:endCharacter` identity.
fn identity_file(identity: &str) -> &str {
    identity.rsplitn(5, ':').last().unwrap_or(identity)
}

/// Declaration identities reached by the definition chain at a position.
pub(in super::super) async fn identities(
    client: &NativeLspClient,
    sources: &mut SourceCache<'_>,
    snippet_policy: &SnippetReadPolicy,
    cancel: &dyn CancellationCheck,
    path: &str,
    line: u32,
    character: u32,
) -> Result<Option<HashSet<String>>, LspFailure> {
    match resolve_definition_chain(
        client,
        sources,
        snippet_policy,
        cancel,
        path,
        line,
        character,
    )
    .await
    {
        Ok((found, warnings, _)) if warnings.is_empty() => {
            Ok(Some(found.iter().map(snippet_identity).collect()))
        }
        // A retained pre-failure alias is not terminal declaration proof.
        Ok(_) => Ok(None),
        Err(failure) if failure.is_cancelled() => Err(failure),
        Err(_) => Ok(None),
    }
}
