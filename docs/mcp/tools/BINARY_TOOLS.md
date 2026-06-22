# Binary Tools Reference

Reference for `localBinaryInspect` — the Octocode MCP tool for inspecting archives, compressed streams, and native binaries.

Requires `ENABLE_LOCAL=true`.

---

## `localBinaryInspect`

Inspect binary files without writing code. Pick the mode for the job:

| Mode | Input | Output |
|------|-------|--------|
| `inspect` | Native binary / object (.so, .dylib, .node, .exe, .dll, .wasm, .o; ELF/Mach-O/PE) or any file | Format, arch, bits, endianness, stripped, symbols, imports, exports, sections, dynamic deps (+ type + magic bytes) |
| `list` | Archive (.zip, .tar.gz, .jar, .7z, …) | Entry names, sizes, timestamps |
| `extract` | Archive + entry name (from `list`) | Entry content |
| `decompress` | Single-stream compressed file (.gz, .bz2, .xz, .zst, .lz4, .br, .lzfse) | Decompressed text |
| `strings` | Native binary (.so, .dylib, .node, .exe, .wasm) | Readable strings (ASCII + UTF-16), symbols, URLs |

`inspect` and `strings` are fully native (octocode-engine / `goblin`) — no `file`, `xxd`, `strings`, or binutils dependency, so they work identically on Windows and on distroless/Alpine Linux.

### Decision Flow

```text
Unknown / native binary → inspect
Archive                 → list  → extract (one entry)
Compressed              → decompress
Want raw readable text  → strings
```

---

## Parameters

### Required

| Parameter | Description |
|-----------|-------------|
| `path` | File path (absolute or workspace-relative). |
| `mode` | One of: `inspect`, `list`, `extract`, `decompress`, `strings`. |

### `inspect` mode

Takes no parameters beyond `path`. Returns identity (`format`, `description`, `magicBytes`) for any file, plus — for recognized executables — `arch`, `bits`, `endianness`, `stripped`, `entry`, `symbolCount`/`importCount`/`exportCount`, and capped `symbols`/`imports`/`exports`/`sections`/`libraries` lists.

### `list` mode

| Parameter | Default | Description |
|-----------|---------|-------------|
| `verbose` | `false` | Include entry size and mtime. |
| `maxEntries` | `1000` | Cap entries before pagination. |
| `entriesPerPage` | unset | Entries per page. Pair with `entryPageNumber`. |
| `entryPageNumber` | `1` | Page for large archives. |

### `extract` mode

| Parameter | Description |
|-----------|-------------|
| `archiveFile` | Exact entry path (case-sensitive, no leading `-`). **Required.** Run `list` first. |
| `matchString` | Filter decompressed lines by this string. |
| `matchStringContextLines` | Lines around each match. Default `3`. |
| `charOffset` | Char offset for content pagination. |
| `charLength` | Max chars to return. Max `50000`. |

### `decompress` mode

| Parameter | Default | Description |
|-----------|---------|-------------|
| `format` | `auto` | Force compression format: `gzip`, `bzip2`, `xz`, `lzma`, `zstd`, `lz4`, `brotli`, `lzfse`. |
| `matchString` | — | Filter decompressed lines. |
| `matchStringContextLines` | `3` | Context lines around each match. |
| `charOffset` | — | Continuation offset for pagination (from `hints[]`). |
| `charLength` | — | Max chars per page. |

### `strings` mode

| Parameter | Default | Description |
|-----------|---------|-------------|
| `minLength` | `8` | Minimum printable run length. Raise (12–16) to surface symbols/URLs only. |
| `includeOffsets` | `false` | Prefix each string with its absolute hex byte offset. |
| `scanOffset` | `0` | Absolute byte offset to start the scan window. Follow the returned `nextScanOffset` cursor to page through a large binary losslessly. |

Recovers both ASCII and UTF-16 (LE/BE) runs — the wide strings GNU `strings -a` misses.

**Lossless scan pagination.** Each call scans a 64MB window; it never discards the tail of a large binary. When more of the file remains, the result carries `nextScanOffset` (an absolute byte offset) — re-call with `scanOffset` set to it to keep scanning. The window is rewound to a safe break, so **no string is ever split across a window boundary** and there are no duplicates. `nextScanOffset` is absent at EOF.

---

## Supported Formats

**Archives (list / extract):** `.zip`, `.jar`, `.war`, `.apk`, `.tar.gz`, `.tgz`, `.tar.bz2`, `.tbz2`, `.tar.xz`, `.txz`, `.tar.zst`, `.tzst`, `.7z`

**Compressed streams (decompress):** `.gz`, `.bz2`, `.xz`, `.lzma`, `.zst`, `.lz4`, `.br`, `.lzfse`

**Native binaries (inspect / strings):** `.so`, `.dylib`, `.node`, `.exe`, `.dll`, `.wasm`, `.o`, any ELF / Mach-O / PE binary or ar archive

> `decompress` rejects multi-entry archives. Use `list`/`extract` for `.tar.gz`, `.zip`, etc.

---

## Examples

```bash
# What is this binary? (format, arch, symbols, imports, exports, deps)
localBinaryInspect(path="dist/server.node", mode="inspect")

# List entries in a zip
localBinaryInspect(path="build.zip", mode="list", verbose=true)

# List a large archive page by page
localBinaryInspect(path="release.tar.gz", mode="list", entriesPerPage=50, entryPageNumber=2)

# Extract one entry (use exact path from list output)
localBinaryInspect(path="build.zip", mode="extract", archiveFile="dist/index.js")

# Extract and filter for a specific function
localBinaryInspect(path="build.zip", mode="extract", archiveFile="dist/index.js",
  matchString="createServer", matchStringContextLines=10)

# Decompress a gzip file
localBinaryInspect(path="report.txt.gz", mode="decompress")

# Decompress and paginate large content
localBinaryInspect(path="report.txt.gz", mode="decompress", charLength=10000)
# → response hints[] contains charOffset=N for next page

# Extract symbols from a native addon
localBinaryInspect(path="packages/addon/bin/addon.node", mode="strings", minLength=12)

# Extract strings with byte offsets (for binary diffing)
localBinaryInspect(path="binary.exe", mode="strings", includeOffsets=true)
```

---

## Bulk Queries

Up to 5 queries per call:

```bash
localBinaryInspect(queries=[
  { path="a.zip", mode="list" },
  { path="b.tar.gz", mode="list" }
])
```

---

## Requirements

- `ENABLE_LOCAL=true`
- `inspect` and `strings` need **no** external CLI — they run natively in octocode-engine (works on Windows / distroless / Alpine).
- Container modes shell out: `list`/`extract`/`unpack` need `unzip`, `tar`, `7z` (or `7zz`/`bsdtar` as fallbacks); `decompress` of `.lz4`/`.br`/`.lzfse` needs `lz4cat`/`brotli`/`lzfse`.
- `extract`, `decompress`, and `strings` return `localPath` when they write derived text to `<octocode-home>/tmp/binary/`; `unpack` writes extracted trees to `<octocode-home>/tmp/unzip/`.

---

## See Also

- [Local Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LOCAL_TOOLS.md)
- [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CONFIGURATION.md)
