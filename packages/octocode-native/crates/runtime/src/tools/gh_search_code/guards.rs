//! Query shapes ghSearchCode refuses before any request: a ref without a repository and
//! glob syntax in `filename` or `path`, each with its repair.
use super::{GhSearchCodeQuery, code_output};
use crate::id::ToolId;
use crate::response::result::{Continuation, query_row};
use crate::tools::gh_shared::GhFailure;
use octocode_engine::search::glob::GLOB_META;
use octocode_github::{ProviderError, ProviderErrorKind};
use serde_json::json;

/// `ref` re-reads one repository's hits at that ref; an owner-wide search
/// spans many repositories, each read at its own indexed commit, so the
/// ref cannot apply. Rejected before any request, with the same search
/// without it as the repair.
pub(super) fn ref_without_repo(query: &GhSearchCodeQuery) -> Option<GhFailure> {
    let reference = code_output::requested_ref(query).filter(|_| query.repo.is_none())?;
    let error = ProviderError::new(
        ProviderErrorKind::Validation,
        format!(
            "ref \"{reference}\" needs repo: an owner-wide search reads each hit at its repository's indexed commit."
        ),
    );
    let message = error.message.to_string();
    let repair = Continuation::new(ToolId::GhSearchCode, query_row(query, &["ref"])).build();
    Some(GhFailure {
        next: Some(Box::new(json!({ "repair": repair }))),
        ..GhFailure::new(error, message).hint("Add repo to read that repository's hits at the ref.")
    })
}

/// GitHub's `filename:` matches file names containing its text, never a
/// glob, so a glob there silently matches nothing. Refused before any
/// request: one whose only glob characters are leading or trailing `*`
/// (`*.test.js`) repairs to the text between them; any other names the
/// rule.
pub(super) fn filename_glob(query: &GhSearchCodeQuery) -> Option<GhFailure> {
    let filename = query.filename.as_deref()?.as_str();
    if !filename.contains(GLOB_META) {
        return None;
    }
    let text = filename.trim_matches('*');
    let repair = (!text.is_empty() && !text.contains(GLOB_META)).then(|| {
        let mut row = query_row(query, &[]);
        row["filename"] = json!(text);
        Continuation::new(ToolId::GhSearchCode, row).build()
    });
    let rule = format!(
        "filename \"{filename}\" is a glob; filename matches file names containing its text"
    );
    let (message, hint) = match &repair {
        Some(_) => (format!("{rule}: use \"{text}\"."), None),
        None => (
            format!("{rule}, without wildcards."),
            Some("Name text the file name contains, or set extensions for its extension."),
        ),
    };
    let error = ProviderError::new(ProviderErrorKind::Validation, message.clone());
    let failure = GhFailure {
        next: repair.map(|repair| Box::new(json!({ "repair": repair }))),
        ..GhFailure::new(error, message)
    };
    Some(match hint {
        Some(hint) => failure.hint(hint),
        None => failure,
    })
}

/// `path` is a directory prefix, never a glob, so a glob there silently
/// matches nothing (and an empty's leads would carry it). Refused before
/// any request: trailing `**`/`*` segments repair to the directory before
/// them, and a last `*.ext` segment to `extensions:["ext"]`; any other glob
/// names the rule.
pub(super) fn path_glob(query: &GhSearchCodeQuery) -> Option<GhFailure> {
    let path = query.path.as_deref()?.as_str();
    if !path.contains(GLOB_META) {
        return None;
    }
    let segments = path.trim_matches('/').split('/').collect::<Vec<_>>();
    let first_glob = segments.iter().position(|s| s.contains(GLOB_META))?;
    let (dir, globbed) = segments.split_at(first_glob);
    let (last, middle) = globbed.split_last()?;
    let extension = last
        .strip_prefix("*.")
        .filter(|ext| !ext.is_empty() && !ext.contains(GLOB_META) && !ext.contains('.'));
    let repairable = middle.iter().all(|s| matches!(*s, "*" | "**"))
        && (matches!(*last, "*" | "**") || extension.is_some())
        && extension.is_none_or(|ext| {
            query.extensions.is_empty() || query.extensions.iter().any(|e| e.as_str() == ext)
        });
    let dir = dir.join("/");
    let repair = repairable.then(|| {
        let mut row = query_row(query, &["path"]);
        if !dir.is_empty() {
            row["path"] = json!(dir);
        }
        if let Some(ext) = extension.filter(|_| query.extensions.is_empty()) {
            row["extensions"] = json!([ext]);
        }
        Continuation::new(ToolId::GhSearchCode, row).build()
    });
    let rule = format!("path \"{path}\" is a glob; path is a directory prefix");
    let message = match (&repair, extension) {
        (Some(_), Some(ext)) if dir.is_empty() => {
            format!("{rule}: drop path, set extensions [\"{ext}\"].")
        }
        (Some(_), Some(ext)) => format!("{rule}: use \"{dir}\" with extensions [\"{ext}\"]."),
        (Some(_), None) if dir.is_empty() => format!("{rule}: drop path."),
        (Some(_), None) => format!("{rule}: use \"{dir}\"."),
        (None, _) => format!("{rule}, without wildcards."),
    };
    let error = ProviderError::new(ProviderErrorKind::Validation, message.clone());
    Some(GhFailure {
        next: repair.map(|repair| Box::new(json!({ "repair": repair }))),
        ..GhFailure::new(error, message)
    })
}
