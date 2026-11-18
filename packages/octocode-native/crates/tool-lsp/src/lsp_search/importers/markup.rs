//! Script-bearing markup (Vue, Svelte, Astro, MDX, HTML): no registry grammar
//! parses it and a TypeScript server reads it only through a plugin, so an
//! importer there is outside every answer that did not list it. A row never
//! reads complete past one: it stays partial and leads to those files. Markup
//! that holds no use of this declaration (prose, an import of a same-named
//! export of another module) changes nothing.
use super::reach::{AnchorModule, read_checked};
use super::*;
use octocode_engine::signatures::markup::{markup_reads, markup_scripts};
use octocode_engine::text::extension_of;
use octocode_runtime_base::response::paging::mark_partial;

/// Whether readable markup `content` of `file` uses `symbol`: an import that
/// binds it from a module that may be the anchor's, or, with no module export
/// to bind (a class member, a script's global), a script or template read.
pub(super) fn uses(
    file: &str,
    content: &str,
    symbol: &str,
    module: Option<&AnchorModule>,
    relays: &mut HashMap<std::path::PathBuf, bool>,
) -> bool {
    let extension = extension_of(file, true, "");
    let Some(module) = module else {
        return markup_reads(content, &extension, symbol);
    };
    let member = format!(".{symbol}");
    markup_scripts(content, file, &extension)
        .imports
        .iter()
        .any(|import| {
            let binds = import.imported == symbol
                || (matches!(import.imported.as_str(), "*" | "default")
                    && octocode_engine::text::whole_word_starts(content, &member)
                        .next()
                        .is_some());
            binds && module.names(&import.specifier, file, true, relays)
        })
}

/// Mark `row` partial for the markup files under the scope that use the
/// query's symbol and that its answer does not cover. `anchor_text` is the
/// anchor file's source, which says whether the symbol is a module export.
pub(in super::super) async fn flag_markup_importers(
    row: &mut Value,
    query: &LspSearchQuery,
    language_id: Option<&str>,
    (scope, anchor, anchor_text): (&Scope, &str, Option<&str>),
    policy: &PathPolicy,
    cancel: &dyn CancellationCheck,
) -> Result<(), LspFailure> {
    let Some(name) = lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty())
    else {
        return Ok(());
    };
    // Markup cannot name a local binding of another file either.
    if !applies(language_id, lsp_search_query::operation(query))
        || row.get("status").and_then(Value::as_str) == Some("error")
        || scope.confined(name)
    {
        return Ok(());
    }
    let unanswered = scope.unanswered_markup(name);
    if unanswered.is_empty() {
        return Ok(());
    }
    let module = anchor_text
        .and_then(|text| AnchorModule::of(anchor, text, name))
        .map(|module| module.in_scope(scope, name, policy));
    let (symbol, policy_owned) = (name.to_owned(), policy.clone());
    let users = crate::lsp_search::blocking_cancellable(cancel, move |stopped| {
        let mut relays = HashMap::new();
        unanswered
            .into_iter()
            .take_while(|_| !stopped())
            .filter(|file| {
                // A file the check cannot read may hold a use it did not see.
                read_checked(&policy_owned, file).is_none_or(|content| {
                    uses(file, &content, &symbol, module.as_ref(), &mut relays)
                })
            })
            .collect::<Vec<_>>()
    })
    .await?;
    cancel.check().map_err(LspFailure::cancelled)?;
    let Some(users) = users.filter(|users| !users.is_empty()) else {
        return Ok(());
    };
    let family = scope.text_only_files(name).map_or(0, |files| files.len());
    super::super::failure::record_text_only(row, scope, name, family + users.len());
    mark_partial(
        row,
        super::super::server_coverage::UNRESOLVED_IMPORT_REASON,
        &[format!(
            "{} markup file(s) (Vue, Svelte, Astro, MDX, HTML) use this name; the server does not read them, so those uses are missing from the answer.",
            users.len()
        )],
    );
    if let Some(lead) =
        super::super::failure::text_search_lead(name, &scope.root, &[], Some(&users))
    {
        octocode_runtime_base::response::result::attach_related(row, lead.build());
    }
    Ok(())
}
