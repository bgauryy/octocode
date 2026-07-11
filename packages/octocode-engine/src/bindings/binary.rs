use crate::bindings::tasks::{ExtractBinaryStringsTask, InspectBinaryTask};
use napi::bindgen_prelude::AsyncTask;
use napi_derive::napi;

/// Native binary inspection (format lane). Parses `path` as an executable /
/// object / archive and returns its identity plus — for recognized executable
/// formats — symbols, imports, exports, sections and dynamic dependencies.
///
/// Replaces the `file` + `xxd` shell-outs. Never throws on malformed input: a
/// parse failure degrades to magic-byte identity with an explanatory note. The
/// only `Err` cases are unreadable / oversized files.
///
/// `goblin` is explicitly not hardened against malicious input, so this runs
/// on libuv's worker pool with a `catch_unwind` guard around the parse (see
/// `InspectBinaryTask`) — an unwind across the napi FFI boundary would abort
/// the Node process. Returns a Promise from JavaScript.
#[napi(js_name = "inspectBinaryNative")]
pub fn inspect_binary_native(path: String) -> AsyncTask<InspectBinaryTask> {
    AsyncTask::new(InspectBinaryTask { path })
}

/// Native strings extraction. Recovers printable ASCII **and** UTF-16 (LE/BE)
/// runs of at least `min_length` from the scan window of `path` beginning at
/// `scan_offset`, longest-first, optionally hex offset-prefixed. Replaces the
/// `strings` shell-out and additionally surfaces the wide strings GNU
/// `strings -a` misses.
///
/// Lossless pagination: the returned `nextScanOffset` (when set) is the absolute
/// byte offset of the next window, rewound to a safe break so no string is split
/// across windows. Pass `scanOffset = 0` for the first window.
///
/// Runs on libuv's worker pool (MB-scale scan) — returns a Promise from
/// JavaScript.
#[napi(js_name = "extractBinaryStringsNative")]
pub fn extract_binary_strings_native(
    path: String,
    min_length: u32,
    include_offsets: bool,
    scan_offset: u32,
) -> AsyncTask<ExtractBinaryStringsTask> {
    AsyncTask::new(ExtractBinaryStringsTask {
        path,
        min_length,
        include_offsets,
        scan_offset,
    })
}
