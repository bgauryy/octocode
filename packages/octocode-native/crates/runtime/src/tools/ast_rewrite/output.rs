//! Response-shaping helpers: build the tool output Value from prepared rewrite data.
use super::{ExecutableReceipt, PreparedFile, PreparedMatch, RewriteError, RewriteRequest};
use crate::tools::id::ToolId;
use serde_json::{Map, Value, json};
use std::path::Path;

pub(super) fn success_value(
    query: &RewriteRequest,
    root: &Path,
    snapshot: &str,
    files: &[PreparedFile],
    matches: &[PreparedMatch],
    executable: &ExecutableReceipt,
    transaction: Option<Value>,
) -> Value {
    let apply = query.apply();
    let page = if apply { 1 } else { query.page() };
    let page_size = if apply {
        matches.len().max(1)
    } else {
        query.page_size()
    };
    let offset = page.saturating_sub(1).saturating_mul(page_size);
    let shown = if apply {
        matches
    } else {
        matches
            .get(offset..offset.saturating_add(page_size).min(matches.len()))
            .unwrap_or(&[])
    };
    let has_more = !apply && offset.saturating_add(page_size) < matches.len();
    let total_pages = matches.len().div_ceil(page_size).max(1);
    // A preview page carries only the files its matches touch, and each
    // file's patch holds only the hunks of this page's matches, so pageSize
    // bounds the patch too. The patch applies to the original file on its
    // own; `beforeHash`/`afterHash` and `matchCount` still describe the whole
    // file, and the final page's `next.apply` guards every affected file.
    let shown_ids = shown
        .iter()
        .map(|matched| matched.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let page_files = files
        .iter()
        .filter(|file| {
            apply
                || file
                    .matches
                    .iter()
                    .any(|matched| shown_ids.contains(matched.id.as_str()))
        })
        .map(|file| {
            let mut value = public_file(file, query.debug());
            if !apply {
                page_patch(&mut value, file, &shown_ids, query.debug());
            }
            value
        })
        .collect::<Vec<_>>();
    let mut value = json!({
        "mode":if apply {"apply"} else {"preview"},
        "root":root,"snapshot":snapshot,"totalMatches":matches.len(),
        "affectedFiles":files.len(),
        "matches":shown.iter().map(|matched| public_match(matched, query.debug())).collect::<Vec<_>>(),
        "files":page_files,
        "complete":!has_more,"isPartial":has_more,
        "pagination":{"currentPage":page,"totalPages":total_pages,"pageSize":page_size,"hasMore":has_more}
    });
    attach_receipts(&mut value, query, executable);
    if has_more {
        let mut next = continuation_query(query, root);
        next["apply"] = json!(false);
        next["page"] = json!(page + 1);
        next["pageSize"] = json!(page_size);
        next["snapshot"] = json!(snapshot);
        value["next"] = json!({"nextPage":{
            "tool":ToolId::AstRewrite.as_str(),"query":next,"confidence":"exact"
        }});
    }
    // A complete preview carries its own guarded apply: the same query with the
    // preview snapshot and every file's beforeHash. Running it stays the
    // caller's decision; changed sources or selections are still rejected.
    if !apply && !has_more && !matches.is_empty() {
        let mut next = continuation_query(query, root);
        next["apply"] = json!(true);
        next["page"] = json!(1);
        next["snapshot"] = json!(snapshot);
        next["expectedHashes"] = Value::Object(
            files
                .iter()
                .map(|file| (file.path.clone(), json!(file.before_hash)))
                .collect(),
        );
        value["next"]["apply"] = json!({
            "tool":ToolId::AstRewrite.as_str(),"query":next,"confidence":"exact"
        });
    }
    if let Some(transaction) = transaction {
        value["transaction"] = transaction;
    }
    if query.debug() {
        value["operation"] = json!("rewrite");
    } else if value["next"].get("apply").is_some()
        && let Some(rows) = value["files"].as_array_mut()
    {
        // next.apply.expectedHashes states each file's beforeHash once.
        for row in rows.iter_mut().filter_map(Value::as_object_mut) {
            row.remove("beforeHash");
        }
    }
    value
}

pub(super) fn continuation_query(query: &RewriteRequest, canonical_root: &Path) -> Value {
    let mut value = Map::new();
    value.insert("path".to_owned(), json!(canonical_root));
    value.insert("langType".to_owned(), json!(query.lang()));
    value.insert("apply".to_owned(), json!(query.apply()));
    value.insert("maxFiles".to_owned(), json!(query.max_files()));
    value.insert("maxMatches".to_owned(), json!(query.max_matches()));
    value.insert("page".to_owned(), json!(query.page()));
    value.insert("pageSize".to_owned(), json!(query.page_size()));
    value.insert("ruleKind".to_owned(), json!(query.rule_kind()));
    for (key, item) in [
        (
            "pattern",
            query.pattern().as_ref().map(|value| json!(value)),
        ),
        (
            "rewrite",
            query.rewrite().as_ref().map(|value| json!(value)),
        ),
        ("rule", query.rule().cloned()),
        ("constraints", query.constraints().cloned()),
        ("utils", query.utils().cloned()),
        ("transform", query.transform().cloned()),
        ("fix", query.fix().cloned()),
        (
            "include",
            query.include().as_ref().map(|value| json!(value)),
        ),
        (
            "exclude",
            query.exclude().as_ref().map(|value| json!(value)),
        ),
        (
            "expectedHashes",
            query.expected_hashes().as_ref().map(|value| json!(value)),
        ),
        (
            "selectedMatchIds",
            query
                .selected_match_ids()
                .as_ref()
                .map(|value| json!(value)),
        ),
        (
            "postconditions",
            query.postconditions().as_ref().map(|value| json!(value)),
        ),
        (
            "snapshot",
            query.snapshot().as_ref().map(|value| json!(value)),
        ),
    ] {
        if let Some(item) = item {
            value.insert(key.to_owned(), item);
        }
    }
    Value::Object(value)
}

/// Narrow a preview file's whole-file patch to the hunks of the page's
/// matches. A page showing every match of the file keeps the whole patch.
fn page_patch(
    value: &mut Value,
    file: &PreparedFile,
    shown: &std::collections::BTreeSet<&str>,
    debug: bool,
) {
    let on_page = file
        .matches
        .iter()
        .filter(|matched| shown.contains(matched.id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if on_page.len() == file.matches.len() {
        return;
    }
    // The full edit set already applied cleanly; a subset of it cannot fail.
    let Ok(after) = super::apply_edits(&file.before, &on_page) else {
        return;
    };
    let (Ok(before), Ok(after)) = (
        std::str::from_utf8(&file.before),
        std::str::from_utf8(&after),
    ) else {
        return;
    };
    let patch = super::create_unified_patch(&file.path, before, after);
    if debug {
        value["patchBytes"] = json!(patch.len());
    }
    value["patch"] = json!(patch);
    value["patchMatchCount"] = json!(on_page.len());
}

/// Hex digits of a match id shown in a preview row; `selectedMatchIds`
/// accepts any unique prefix of at least 12.
pub(super) const MATCH_ID_PREFIX: usize = 16;

/// A preview match row locates its hunk: id prefix, path and 1-based line.
/// The patch already shows the text and its replacement; `debug` keeps the
/// full row (64-hex id, ranges, text, replacement, captures).
fn public_match(matched: &PreparedMatch, debug: bool) -> Value {
    if debug {
        return matched.public.clone();
    }
    json!({
        "id":&matched.id[..MATCH_ID_PREFIX.min(matched.id.len())],
        "path":matched.public["path"],
        "line":matched.public["range"]["start"]["line"]
    })
}

/// A file row: `beforeHash` guards apply (a complete preview states it once,
/// in `next.apply.expectedHashes`) and `matchCount` sizes the edit.
/// `afterHash` (recomputed by the journal), `patchBytes` and `absolutePath`
/// are diagnostics kept under `debug`.
pub(super) fn public_file(file: &PreparedFile, debug: bool) -> Value {
    let mut value = json!({
        "path":file.path,"beforeHash":file.before_hash,
        "matchCount":file.matches.len(),"patch":file.patch
    });
    if debug {
        value["absolutePath"] = json!(file.absolute);
        value["afterHash"] = json!(file.after_hash);
        value["patchBytes"] = json!(file.patch.len());
    }
    value
}

pub(super) fn executable_value(executable: &ExecutableReceipt) -> Value {
    let mut value = json!({
        "path":executable.path,
        "version":executable.version,
        "capabilityContract":1,
        "capabilityDigest":executable.capability_digest,
        "capabilities":executable.capabilities
    });
    if !executable.sha256.is_empty() {
        value["sha256"] = json!(executable.sha256);
    }
    value
}

/// The executable and isolation receipts are diagnostics: the snapshot digest
/// already binds the executable, and nothing a caller does next depends on
/// them, so they ride only on `debug`.
pub(super) fn attach_receipts(
    value: &mut Value,
    query: &RewriteRequest,
    executable: &ExecutableReceipt,
) {
    if query.debug() {
        value["executable"] = executable_value(executable);
        value["isolation"] = isolation_receipt();
    }
}

pub(super) fn isolation_receipt() -> Value {
    json!({
        "workingDirectory":"ephemeral","inheritedHome":false,
        "repositoryConfig":"not-discovered"
    })
}

pub(super) fn portable_relative(root: &Path, target: &Path) -> Result<String, RewriteError> {
    target
        .strip_prefix(root)
        .map(|path| {
            path.to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/")
        })
        .map_err(|_| {
            RewriteError::new(
                "ast.rewrite.path_escape",
                "The rewrite escaped the requested root.",
            )
        })
}
