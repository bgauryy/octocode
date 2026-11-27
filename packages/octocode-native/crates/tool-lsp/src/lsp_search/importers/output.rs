//! A candidate site bound to the anchor package's build output: a
//! workspace package imported by name resolves to its emitted `.d.ts`, so
//! the server's definition chain stops on that declaration, never on the
//! anchor's source (see [`BuildOutput`]).
use super::*;
use crate::lsp_search::build_output::BuildOutput;

/// A site's definition-chain identities once the anchor package's build
/// output stands for its source.
pub(super) struct Translated {
    pub(super) resolved: HashSet<String>,
    /// An identity lay in the anchor package's build output and no source
    /// declaration of the anchor's file could be matched to it (a stale or
    /// unmappable build): the site binds that output, unverified.
    pub(super) unmapped: bool,
}

/// `resolved` with each identity in `output` (the anchor package's build
/// output) replaced by the anchor's own `declaration` identities when it
/// is their compiled mirror: it lies in the declaration file compiled from
/// a file that holds them, and names the same identifier there. An output
/// identity that mirrors nothing is dropped and reported as `unmapped`.
pub(super) async fn through_output(
    resolved: HashSet<String>,
    output: Option<&BuildOutput>,
    declaration: &HashSet<String>,
    sources: &mut SourceCache<'_>,
) -> Translated {
    let Some(output) = output else {
        return Translated {
            resolved,
            unmapped: false,
        };
    };
    let mut translated = HashSet::new();
    let mut unmapped = false;
    for identity in resolved {
        let Some((file, line, character)) = identity_start(&identity) else {
            translated.insert(identity);
            continue;
        };
        if !output.holds(Path::new(file)) {
            translated.insert(identity);
            continue;
        }
        let mirrored = match output.source_of(Path::new(file)) {
            Some(source) => {
                let name = name_at(sources, file, line, character).await;
                let mut found = Vec::new();
                for target in declaration {
                    let Some((target_file, target_line, target_character)) = identity_start(target)
                    else {
                        continue;
                    };
                    if Path::new(target_file) == source
                        && name.is_some()
                        && name_at(sources, target_file, target_line, target_character).await
                            == name
                    {
                        found.push(target.clone());
                    }
                }
                found
            }
            None => Vec::new(),
        };
        if mirrored.is_empty() {
            unmapped = true;
        }
        translated.extend(mirrored);
    }
    Translated {
        resolved: translated,
        unmapped,
    }
}

/// The file and zero-based start of a `path:line:character:endLine:endCharacter`
/// identity.
fn identity_start(identity: &str) -> Option<(&str, u32, u32)> {
    let mut parts = identity.rsplitn(5, ':');
    let (_, _, character, line, file) = (
        parts.next()?,
        parts.next()?,
        parts.next()?,
        parts.next()?,
        parts.next()?,
    );
    Some((file, line.parse().ok()?, character.parse().ok()?))
}

/// The identifier that starts at zero-based `line` and UTF-16 `character`
/// of `file`; `None` when the file cannot be read or no identifier starts
/// there.
async fn name_at(
    sources: &mut SourceCache<'_>,
    file: &str,
    line: u32,
    character: u32,
) -> Option<String> {
    let source = sources.get(file).await?;
    crate::lsp_search::build_output::identifier_at(&source.content, line, character)
}

#[cfg(test)]
mod tests {
    use super::identity_start;

    #[test]
    fn an_identity_names_its_file_and_start() {
        assert_eq!(
            identity_start("/repo/a:b/dist/x.d.ts:31:24:31:39"),
            Some(("/repo/a:b/dist/x.d.ts", 31, 24))
        );
        assert_eq!(identity_start("no-identity"), None);
    }
}
