//! Why a changed file has no provider patch, and the inventory flag that
//! says so.
use crate::tools::gh_get_history_item::util::{str_at, usize_at};
use serde_json::{Value, json};

/// Extensions GitHub never diffs as text.
const BINARY_EXTENSIONS: &[&str] = &[
    "7z", "a", "avi", "bin", "bmp", "bz2", "class", "db", "dll", "dylib", "eot", "exe", "flac",
    "gif", "gz", "ico", "jar", "jpeg", "jpg", "lib", "mov", "mp3", "mp4", "node", "o", "ogg",
    "otf", "pdf", "png", "psd", "pyc", "rlib", "so", "sqlite", "tgz", "tiff", "ttf", "war", "wasm",
    "wav", "webm", "webp", "woff", "woff2", "xz", "zip",
];

/// Why a changed file has no provider patch, or `None` when there is nothing
/// to diff (a pure rename). `tooLarge`: GitHub omits a single oversized diff
/// but still counts its lines. `binary`: a binary extension. `omitted`:
/// GitHub sent neither patch nor line counts — binary, empty, or past the
/// PR's total diff budget — so `additions`/`deletions` of 0 are not evidence
/// of an unchanged file; read the file at `headSha` instead.
pub(in crate::tools::gh_get_history_item) fn missing_patch_reason(
    file: &Value,
) -> Option<&'static str> {
    let changes = usize_at(file, "/additions") + usize_at(file, "/deletions");
    if changes > 0 {
        return Some("tooLarge");
    }
    if str_at(file, "/status") == Some("renamed") {
        return None;
    }
    let name = str_at(file, "/filename").unwrap_or("");
    Some(if is_binary_name(name) {
        "binary"
    } else {
        "omitted"
    })
}

/// Whether a changed file's name has a binary extension (its basename's,
/// never a dotted directory's).
pub(super) fn is_binary_name(path: &str) -> bool {
    BINARY_EXTENSIONS.contains(&octocode_engine::text::extension_of(path, true, "").as_str())
}

/// Inventory rows flag files GitHub sent without a patch. REST entries
/// (which carry a blob `sha`) say why; GraphQL file nodes never include a
/// patch, so only their binary files are flagged (by name: a binary change
/// has no line counts either).
pub(super) fn inventory_patch_flag(row: &mut Value, file: &Value) {
    if file.get("patch").is_some() {
        return;
    }
    let reason = missing_patch_reason(file)
        .filter(|reason| file.get("sha").is_some() || *reason == "binary");
    if let Some(reason) = reason {
        row["patchUnavailable"] = json!(reason);
    }
}
