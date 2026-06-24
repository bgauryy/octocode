# Research Binary — Archives, Compressed Streams, Native Binaries

Use this when a file is an archive, compressed stream, native binary, `.node` addon, `.wasm`, `.jar`, `.zip`, `.tar.*`, `.7z`, `.dmg`, `.deb`, or unknown binary. The artifact lane only opens/unpacks; after unpacking, continue with local research tools.

## CLI ↔ MCP map

| Job | CLI | MCP |
|---|---|---|
| Inspect binary metadata (format/arch/counts/deps) | `octocode search <file> --target artifacts --inspect` | `localBinaryInspect(mode:"inspect")` |
| List archive entries | `octocode search <file> --target artifacts --list` | `localBinaryInspect(mode:"list")` |
| Extract one archive entry | `octocode search <file> --target artifacts --extract <entry>` | `localBinaryInspect(mode:"extract", archiveFile:"entry")` |
| Decompress single stream | `octocode search <file> --target artifacts --decompress` | `localBinaryInspect(mode:"decompress")` |
| Read printable strings | `octocode search <file> --target artifacts --strings` | `localBinaryInspect(mode:"strings")` |
| Unpack full archive | `octocode unzip <archive>` | `localBinaryInspect(mode:"unpack")` |

## Decision flow

```text
unknown file
→ run with no flags (mode auto-detects: archive→list, compressed→decompress, else→inspect)
→ if archive: list
   → one target entry: extract exact entry
   → many files / codebase: unpack → localPath
→ if compressed stream: decompress
→ if native binary: inspect for metadata, strings for printable runs
→ if unpacked localPath: localViewStructure → localSearchCode/AST → localGetFileContent → LSP if source files exist
```

## Modes

### `inspect`

Default for native binaries and unknown non-archive files. Native parse of
ELF/Mach-O/PE/COFF/ar/wasm → identity, format, arch, counts, and dynamic deps
(no external binutils). Add `--detailed` (`detailed:true`) only when you need the
full symbol/import/export/section arrays.

### `list`

Use on archives before `extract`.

Options:
- `entriesPerPage`, `entryPageNumber` — page entry list.
- `verbose:true` — include size/mtime.
- `maxEntries` — cap returned entries while preserving total count.

### `extract`

Use for one exact archive entry.

Rules:
- Run `list` first; do not guess `archiveFile`.
- Entry is case-sensitive and must not start with `-`.
- Use `matchString` to filter streamed content when needed.
- For many entries, use `unpack` instead.

### `decompress`

Use for single-stream compression: `.gz`, `.bz2`, `.xz`, `.zst`, `.lz4`, `.br`, `.lzfse`.

Rules:
- Not for multi-entry archives.
- `format:"auto"` is default; override only when extension lies.
- Use `matchString` to filter lines during decompression.

### `strings`

Use on `.node`, `.so`, `.dylib`, `.exe`, `.wasm`, native libraries.

Options:
- `minLength:12|16` (CLI `--min-length`) — reduce noise to URLs/symbols/version strings.
- `includeOffsets:true` (CLI `--offsets`) — include byte offsets for follow-up binary investigation.

`--json` output shape: strings land in `data.content` (a string), not a
`strings[]` array. Companion fields are `totalFound`, `contentLength`,
`isPartial`, `scanOffset`, and `pagination`. Read `data.content` and, when
`isPartial`, page the rest with `scanOffset`/`charOffset`. To find a specific
term in a binary strings artifact, follow the returned `next.search` query or
run `octocode search <pattern> <strings-file>` over the extracted strings file
instead of scanning every string manually.

### `unpack`

Use for archives that contain many source files.

Output:
- `localPath` under the octocode archive cache.

Then continue:

```text
octocode search <localPath> --tree --depth 1 --json
→ octocode search <query> <localPath> --search path --json
→ octocode search <kw> <localPath> --view discovery --json
→ octocode search <localPath> --pattern/--rule ... --lang <language> when source grammar exists
→ octocode search <path> --content-view symbols / --match-string ... --content-view exact --json
→ octocode search <file> --op ... if language server can handle unpacked source
```

## Gotchas

- `extract` on `.tar.*` / `.7z` may require `7z` on PATH; `.zip` usually does not.
- Binary inspection is not full-text search across entries. Unpack first, then use local search.
- Never LSP an archive path directly. LSP needs real source files on disk.
- Treat `strings` findings as hints; prove behavior with source, exported symbols, docs, or runtime tests.
- If content is sanitized/redacted, cite that caveat in findings.

---

## Docs

- [Binary Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/BINARY_TOOLS.md)
