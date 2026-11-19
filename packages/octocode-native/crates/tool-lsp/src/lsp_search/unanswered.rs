//! The text check behind an incoming answer: which files spell the name but are
//! not in it, and, where a server has no signal of its own, which of those it
//! never loaded.
use super::*;
use octocode_engine::lsp::client::SnippetReadPolicy;

/// What one page's check reads.
pub(super) struct Request<'a> {
    pub(super) query: &'a LspSearchQuery,
    pub(super) target: &'a Target,
    pub(super) session: &'a Session,
    pub(super) scope: &'a scope::Scope,
    pub(super) anchor: &'a anchor::Anchor,
    pub(super) snippet_policy: &'a SnippetReadPolicy,
    pub(super) paths: &'a PathPolicy,
    pub(super) cancel: &'a dyn CancellationCheck,
}

/// Scan the scope for the name and settle each file the answer leaves out, for
/// `server_coverage::annotate`. An answer listing uses speaks for the server's own
/// project, so with no other project depending on it (by build files) a server
/// without a signal of its own needs no scan at all.
///
/// The scan reads what the ignore rules let a text search see, as ripgrep and
/// editor search do: gitignored and hidden files are out of scope.
pub(super) async fn check(
    request: Request<'_>,
    sources: &mut SourceCache<'_>,
    result: &Value,
) -> Result<(), Exit> {
    let Request {
        query,
        target,
        session,
        scope,
        anchor,
        snippet_policy,
        paths,
        cancel,
    } = request;
    let language_id = target.config.language_id.as_deref();
    let Some(name) =
        lsp_search_query::symbol_name(query).filter(|_| needs_text_scan(query, result))
    else {
        return Ok(());
    };
    let lacks_signal = server_coverage::lacks_signal(language_id);
    let lists_uses = !server_coverage::lists_no_use(result);
    if lists_uses && (lacks_signal || language_id == Some("rust")) {
        let extension = Path::new(&target.path)
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dependents = scope
            .dependents(
                &target.config.workspace_root,
                &target.path,
                &extension,
                cancel,
            )
            .await?;
        if lacks_signal && dependents.is_empty() {
            return Ok(());
        }
    }
    // A declaration whose reach is bounded is checked only where it reaches
    // (TE11): the rest of the scope is out of reach unread.
    scope.text_files_in_reach(name, paths, cancel).await?;
    // A server with no signal of its own indexed its whole project, and a project
    // that does not depend on it cannot use the name: a file of either that the
    // answer leaves out holds no use.
    if lacks_signal && lists_uses {
        server_coverage::answer_settled_files(scope, name, &target.config.workspace_root);
    }
    scope.check_code_uses(name, cancel).await?;
    // Ask the server what each file's spelling names: only one it cannot place
    // elsewhere can be a use it missed. A Rust answer listing uses is asked too:
    // a file of its own project that spells the name may name another symbol
    // (two types with a method of one name), which no file-level reason, such
    // as a package's unloaded build-script output, may be blamed for.
    if (lacks_signal && !lists_uses || language_id == Some("rust"))
        && !target.root_only
        && scope
            .text_only_files(name)
            .is_some_and(|files| !files.is_empty())
    {
        let checked = importers::verified_anchors(
            &session.client,
            sources,
            snippet_policy,
            cancel,
            importers::ImporterSearch {
                symbol: name,
                scope,
                declaration: super::anchor::Position {
                    path: &target.path,
                    line: anchor.line,
                    character: anchor.character,
                },
                known_files: &scope.answered_files(),
                window: importers::WindowRequest::default(),
                proof: importers::SiteProof::DefinitionChain,
                reads_imports: false,
            },
        )
        .await
        .map_err(|failure| session_failed(query, target, session, failure))?;
        scope.answer(checked.rejected);
    }
    Ok(())
}
