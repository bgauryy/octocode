# Positions & URIs — coordinates that survive the boundary

Load when converting between source text and LSP positions, rendering line numbers, or comparing and deduping server-returned locations. Why: off-by-one lines, UTF-16 columns, and symlinked paths are the most common sources of "right answer, wrong place".

## Positions
- LSP `Position` is `{line, character}`, **both 0-based**. `character` counts **UTF-16 code units** by default. Negotiate with `general.positionEncodings` (3.17); the server answers in `capabilities.positionEncoding`, and a missing answer means utf-16.
- `Range.end` is **exclusive**. A whole line is `{l,0}..{l+1,0}`.
- `\n`, `\r\n`, and `\r` are all line breaks. Never split on `\n` alone.
- Compute columns on **exactly the text you sent in `didOpen`**, not a re-read that may differ.
- A column past the end of the line is clamped by the server. Don't rely on that to hide a bad conversion.

### octocode conventions
- Only utf-16 is advertised. A server that names any other encoding fails startup, and an omitted encoding is accepted as utf-16 (`client.rs` `extract_position_encoding` and its start check). Byte→UTF-16 conversion: `resolver.rs` `byte_offset_to_utf16`.
- Line breaks follow LSP (`\r\n`, `\n`, lone `\r`) through one `resolver.rs` `LineIndex`, shared by the resolver, snippet reads, and the runtime.
- Public output lines and UTF-16 columns count from 1 (LSP value + 1); the single conversion point is `R/locations.rs` (`public_range`). The per-field input/output table is in `references/agent-usage.md`.

### Conversion bugs to test for
| Bug | Test fixture |
|---|---|
| Byte offset or Rust `char` count used as the UTF-16 column | A line with `é` and an emoji (surrogate pair) before the symbol |
| 1-based ↔ 0-based applied twice or not at all | Symbol on line 1 and on the last line |
| Inclusive `end` | A single-char identifier |
| `\r` left inside CRLF lines | A CRLF file |

## URIs
- Build and parse with a real library (`url::Url::from_file_path` / `to_file_path`, `uri.rs`). Spaces, `#`, `%`, and non-ASCII must be percent-encoded.
- Windows: a lowercase drive letter and a percent-encoded uppercase one (`c:` vs `C%3A`) name the same file. Compare **decoded, canonical paths**, never raw URI strings.
- Symlinks: canonicalize the workspace root before `initialize`. Also canonicalize every server-returned path before dedup, policy checks ("inside workspace"), and cache keys. On macOS `/tmp` is `/private/tmp`, and `/var` is `/private/var`.
- octocode canonicalizes input files and the workspace root through the read policy (`R/source.rs` `SourceCache`, which calls `fs::canonicalize`), and authorizes server URIs **before** any read (`SnippetReadPolicy`). The engine pool key also canonicalizes (`normalize_workspace_root`), so the napi path gets one server per real root.
- Keep URIs as opaque RFC 3986 strings on the wire. `url::Url` (WHATWG) can re-encode them, and some servers then reject the URI they get back.

Next: for what the positions feed into load `references/primitives.md`.
