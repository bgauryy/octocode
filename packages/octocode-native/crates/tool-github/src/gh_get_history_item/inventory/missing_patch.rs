//! Why a changed file has no provider patch, and the inventory flag that
//! says so.
use crate::gh_get_history_item::util::{str_at, usize_at};
use serde_json::{Value, json};

/// Extensions GitHub never diffs as text.
const BINARY_EXTENSIONS: &[&str] = &[
    "7z", "a", "avi", "bin", "bmp", "bz2", "class", "db", "dll", "dylib", "eot", "exe", "flac",
    "gif", "gz", "ico", "jar", "jpeg", "jpg", "lib", "mov", "mp3", "mp4", "node", "o", "ogg",
    "otf", "pdf", "png", "psd", "pyc", "rlib", "so", "sqlite", "tgz", "tiff", "ttf", "war", "wasm",
    "wav", "webm", "webp", "woff", "woff2", "xz", "zip",
];

/// Git's object ids of the empty blob (SHA-1 and SHA-256 repositories).
const EMPTY_BLOB_IDS: [&str; 2] = [
    "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391",
    "473a0f4c3be8a93681a267e3b1e9a7dcda1185436fe141f7749120a303721813",
];

/// Why a changed file has no patch, or `None` when there is nothing to
/// diff: a pure rename, or an empty file (its blob is git's empty blob, so
/// its 0/0 is the whole change). `withheld`: a credential file whose text
/// the path policy keeps out of every response (the source marked it
/// `withheld`; GitHub did send its patch). `tooLarge`: GitHub omits a
/// single oversized diff but still counts its lines. `binary`: a binary
/// extension. `omitted`: GitHub sent neither patch nor line counts — binary
/// or past the PR's total diff budget — so `additions`/`deletions` of 0 are
/// not evidence of an unchanged file; read the file at `headCommitSha`
/// instead.
pub(in crate::gh_get_history_item) fn missing_patch_reason(file: &Value) -> Option<&'static str> {
    if is_withheld(file) {
        return Some(WITHHELD);
    }
    let changes = usize_at(file, "/additions") + usize_at(file, "/deletions");
    if changes > 0 {
        return Some("tooLarge");
    }
    if str_at(file, "/status") == Some("renamed")
        || str_at(file, "/sha").is_some_and(|sha| EMPTY_BLOB_IDS.contains(&sha))
    {
        return None;
    }
    let name = str_at(file, "/filename").unwrap_or("");
    Some(if is_binary_name(name) {
        "binary"
    } else {
        "omitted"
    })
}

/// Whether a changed file GitHub sent without a patch has source a read
/// reaches: a REST entry (it carries a blob `sha`) whose patch was too large
/// or omitted. A binary file has no text to read, a withheld file's text is
/// kept out by the path policy, and a GraphQL node says nothing about its
/// patch.
pub(in crate::gh_get_history_item) fn source_readable_without_patch(file: &Value) -> bool {
    file.get("patch").is_none()
        && file.get("sha").is_some()
        && matches!(missing_patch_reason(file), Some("tooLarge" | "omitted"))
}

/// The reason a credential file's patch is absent: the path policy, not
/// GitHub, kept it out, so the row is whole as served (no limit, nothing
/// partial).
pub(in crate::gh_get_history_item) const WITHHELD: &str = "withheld";

/// Whether the source withheld a changed file's patch as a credential
/// file's text.
fn is_withheld(file: &Value) -> bool {
    file.get("withheld").and_then(Value::as_bool) == Some(true)
}

/// Whether a changed file's name has a binary extension (its basename's,
/// never a dotted directory's).
pub(super) fn is_binary_name(path: &str) -> bool {
    BINARY_EXTENSIONS.contains(&octocode_engine::text::extension_of(path, true, "").as_str())
}

/// Inventory rows flag files shown without a patch. REST entries (which
/// carry a blob `sha`) say why; GraphQL file nodes never include a patch,
/// so only their binary files are flagged (by name: a binary change has no
/// line counts either). A withheld credential file says so wherever it was
/// read.
pub(super) fn inventory_patch_flag(row: &mut Value, file: &Value) {
    if file.get("patch").is_some() {
        return;
    }
    let reason = missing_patch_reason(file)
        .filter(|reason| file.get("sha").is_some() || matches!(*reason, "binary" | WITHHELD));
    if let Some(reason) = reason {
        row["patchUnavailable"] = json!(reason);
    }
}
