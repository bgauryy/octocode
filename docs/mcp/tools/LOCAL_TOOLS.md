# Local Tools Reference

> Complete reference for Octocode MCP local tools: file system exploration, metadata search, code search, and targeted file reading.

---

## Scope

| Tool | Purpose |
|------|---------|
| `localSearchCode` | Ripgrep content search + AST/structural search (`mode:"structural"`). |
| `localViewStructure` | Browse directory structure and metadata. |
| `localFindFiles` | Find files/directories by name, path, time, size, type, and permissions. |
| `localGetFileContent` | Read targeted file content by line range, match, signature skeleton, or char page. |
| `localBinaryInspect` | Inspect archives, compressed streams, and native binaries. See [BINARY_TOOLS.md](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/BINARY_TOOLS.md). |

---

## Configuration

Local tools must be enabled before use:

```bash
ENABLE_LOCAL=true
```

Or via config:

```json
{
  "local": {
    "enabled": true
  }
}
```

Useful local-tool environment variables:

| Variable | Description |
|----------|-------------|
| `ENABLE_LOCAL` | Enables local filesystem tools. Defaults to `true` for CLI and `false` for MCP. |
| `WORKSPACE_ROOT` | Root used to resolve relative local paths. Overrides `local.workspaceRoot` in config. |
| `ALLOWED_PATHS` | Optional comma-separated allowlist. Empty means all paths are allowed after normal validation. |
| `ENABLE_CLONE` | Enables clone-backed workflows and GitHub directory fetches that materialize local files. |

Config reference: [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CONFIGURATION.md).

---

## Platform Support

`localSearchCode` runs ripgrep in-process through Octocode's native engine. There is no external `rg` binary dependency and no `grep` fallback.

`localGetFileContent` is pure Node.js and works on macOS, Linux, and Windows.

`localFindFiles` and `localViewStructure` use POSIX `find` and `ls`. They work on macOS and Linux out of the box. On Windows, use Git Bash or WSL, or prefer `localSearchCode(filesOnly=true, ...)` when content search can answer the question.

---

## Pagination

All tools accept up to 5 queries per call.

Local tools expose two pagination layers:

| Layer | Fields | Applies To |
|-------|--------|------------|
| Native result pagination | `page`, `itemsPerPage` | `localSearchCode`, `localViewStructure`, `localFindFiles` |
| Per-file match pagination | `matchPage`, `maxMatchesPerFile` | `localSearchCode` when a matched file has more matches |
| Content pagination | `charOffset`, `charLength` | `localGetFileContent` and oversized per-query payloads |
| Bulk response pagination | `responseCharOffset`, `responseCharLength` | Any local-tool bulk response |

Use native pagination first for result lists, then char pagination only when a single result payload is still too large.

---

## Tool Selection

| Need | Use |
|------|-----|
| "Which directories/files exist here?" | `localViewStructure` |
| "Find files named `*.test.ts` or changed recently." | `localFindFiles` |
| "Search for text, regex, imports, TODOs, or identifiers." | `localSearchCode` |
| "Read this exact file section." | `localGetFileContent` |
| "Find files containing a pattern without match bodies." | `localSearchCode(filesOnly=true, ...)` |
| "Find files that do not contain a pattern." | `localSearchCode(filesWithoutMatch=true, ...)` |

Recommended order for code research:

```text
DISCOVER -> SEARCH -> READ
```

Start broad with structure or metadata, narrow with content search, then read the smallest exact file slice needed.

---

## `localSearchCode`

Fast content search powered by ripgrep.

### Best For

- Finding identifiers, imports, route names, constants, TODOs, errors, config keys, and string literals.
- Listing files that contain or do not contain a pattern.
- Getting compact match context before deciding which file section to read.

### Key Parameters

| Parameter | Description |
|-----------|-------------|
| `path` | File or directory to search. Relative paths resolve from the workspace root. |
| `keywords` | Text or regex pattern for non-structural search. Use `fixedString=true` for literal search. Required unless `mode:"structural"`. |
| `mode` | `paginated` (default), `discovery` (file paths only), `detailed` (expanded context), `structural` (AST/shape — use `pattern` or `rule`). |
| `pattern` | Octocode code-shaped AST pattern. `$X` = single node, `$$$ARGS` = list. **Only with `mode:"structural"`**. |
| `rule` | YAML relational rule (`not`/`inside`/`has`/`all`/`any`). Add `stopBy: end` for ancestor/descendant relations. **Only with `mode:"structural"`**. |
| `filesOnly` | Return matching file paths only. |
| `filesWithoutMatch` | Return files that do not match. Mutually exclusive with `filesOnly`. |
| `onlyMatching` | Return only the matched substring(s), one entry per hit, instead of the whole line — the way to enumerate every hit on a minified one-liner. |
| `matchWindow` | With `onlyMatching`, widen each matched span by this many characters of context on each side (… marks trimmed sides). 0 = bare match. Requires `onlyMatching`. |
| `unique` | With `onlyMatching`, collapse to distinct match values (per file) — no manual `sort -u`. Requires `onlyMatching`. |
| `countUnique` | With `onlyMatching`, return distinct match values with a frequency count, sorted most-frequent first. Requires `onlyMatching`. |
| `contextLines` | Lines around each match. Max 100. |
| `matchContentLength` | Max characters per individual match snippet. Default 200, max 100000. |
| `maxFiles` | Hard cap on matched files. |
| `maxMatchesPerFile` | Hard cap on matches per file. Pair with `matchPage` to continue. |
| `page` | Result page across matched files. |
| `matchPage` | Per-file match page when a file has more matches. |

### Matching Options

| Parameter | Description |
|-----------|-------------|
| `fixedString` | Treat `keywords` as literal text. Mutually exclusive with `perlRegex`. |
| `perlRegex` | Enable PCRE2 regex features. Mutually exclusive with `fixedString`. |
| `caseSensitive` | Force case-sensitive matching. |
| `caseInsensitive` | Force case-insensitive matching. Mutually exclusive with `caseSensitive`. |
| `wholeWord` | Match whole words only. |
| `multiline` | Enable cross-line matching. |
| `multilineDotall` | Let `.` match newlines. Requires `multiline=true`. |
| `invertMatch` | Return non-matching lines, or with `filesOnly`, files lacking the pattern. |

### Filters

| Parameter | Description |
|-----------|-------------|
| `langType` | Ripgrep language/type filter such as `ts`, `js`, `py`, `go`. |
| `include` | Glob patterns to include. |
| `exclude` | Glob patterns to exclude. |
| `excludeDir` | Directory names to skip. |
| `hidden` | Include hidden files. |
| `noIgnore` | Ignore `.gitignore` and `.ignore` files. |
| `sort` | Sort by `path`, `modified`, `accessed`, or `created`. |
| `sortReverse` | Reverse the selected sort direction. |

### Output

Normal results include matched files and match snippets with line and column information. For count-only output use `countLinesPerFile:true` or `countMatchesPerFile:true`.

When matches are returned, `localSearchCode` also emits a machine-readable
`next` map for common agent follow-ups:

| Next key | Tool | Purpose |
|----------|------|---------|
| `fetchExact` | `localGetFileContent` | Read the first match with `minify:"none"` for exact source, comments, tests, or edits. |
| `fetchStandard` | `localGetFileContent` | Read a token-efficient source slice with `minify:"standard"`. |
| `fetchSymbols` | `localGetFileContent` | Get a file-level symbol skeleton with `minify:"symbols"`. |
| `lspDefinition` / `lspReferences` | `lspGetSemantics` | Follow the first match semantically when a safe symbol name can be inferred. |
| `nextPage` / `nextMatchPage` | `localSearchCode` | Continue file-level or per-file match pagination. |

### Examples

```bash
localSearchCode(path="packages/octocode-mcp/src", keywords="registerTool", langType="ts")
localSearchCode(path=".", keywords="TODO", filesOnly=true)
localSearchCode(path="src", keywords="class\\s+\\w+Service", perlRegex=true, contextLines=3)
```

### Structural / AST Search

Use `mode:"structural"` for code-shape queries regex cannot express (find all `await` inside `for` loops, calls with N args, functions missing `try/catch`).

**Supported languages:** ts, tsx, js, jsx, mjs, cjs, py, go, rs, java, c/h, cpp/cc/cxx, cs, sh/bash/zsh.

```bash
localSearchCode(path="src", mode="structural", pattern="console.log($$$ARGS)")
# `rule` is a YAML string: \n below are real newline escapes in the JSON tool
# arg (not literal backslash-n). On the CLI, use $'...' or a real multiline string.
localSearchCode(path="src", mode="structural", rule="rule:\n  pattern: await $C\n  inside:\n    kind: for_statement\n    stopBy: end")
localSearchCode(path=".", mode="structural", pattern="eval($X)")
```

---

## `localViewStructure`

Directory browsing for understanding shape, ownership, and file distribution.

### Best For

- Orienting in a new repository.
- Inspecting package/source/test boundaries.
- Finding likely entry points before content search.

### Key Parameters

| Parameter | Description |
|-----------|-------------|
| `path` | Directory to browse. Relative paths resolve from the workspace root. |
| `recursive` | Descend into subdirectories. Use with `maxDepth` to control cost. |
| `maxDepth` | Recursion depth. Max 20. Use low depth first. |
| `page` | Result page. |
| `itemsPerPage` | Directory entries per page. Max 50. |
| `limit` | Hard pre-pagination cap. Max 10000. |
| `filesOnly` | Return files only. |
| `directoriesOnly` | Return directories only. |
| `extensions` | Only include files with selected extensions. |
| `pattern` | Filter entries by glob or substring. |
| `hidden` | Include hidden files and directories. |
| `details` | Include size, permissions, and dates. |
| `showFileLastModified` | Include last-modified timestamps. |
| `sortBy` | Sort field. |
| `reverse` | Reverse sort order. |

### Output

`entries[]` contains structured file/directory entries. The response also includes summary and pagination metadata when applicable.

### Examples

```bash
localViewStructure(path=".", recursive=true, maxDepth=1)
localViewStructure(path="packages/octocode-mcp/src", recursive=true, maxDepth=2, directoriesOnly=true)
localViewStructure(path="docs", extensions=["md"], details=true)
```

---

## `localFindFiles`

Metadata search for files and directories.

### Best For

- Finding files by name, extension, regex, path slice, size, permission, or modified time.
- Locating tests, configs, generated files, or recently changed files.
- Metadata search when content search is not needed.

### Key Parameters

| Parameter | Description |
|-----------|-------------|
| `path` | Directory root for metadata search. |
| `names` | Filename globs OR-combined, such as `["*.ts", "*.tsx"]`. |
| `pathPattern` | Glob matched against the full path. |
| `regex` | Rust regex over the basename only. |
| `entryType` | `f` for files, `d` for directories. |
| `minDepth` / `maxDepth` | Depth bounds. |
| `modifiedWithin` | Files modified within a window, such as `7d` or `2h`. |
| `modifiedBefore` | Files modified before a date/window. |
| `accessedWithin` | Files accessed within a window. |
| `sizeGreater` / `sizeLess` | Size filters such as `100k` or `1m`. |
| `empty` | Empty files/directories only. |
| `permissions` | Permission string filter. |
| `executable` / `readable` / `writable` | Permission predicates. |
| `excludeDir` | Directory names to skip. |
| `details` | Include file metadata. |
| `showFileLastModified` | Include modification timestamps. |
| `sortBy` | Sort by `modified`, `name`, `path`, or `size`. |
| `page` | Result page. |
| `itemsPerPage` | Files per page. Max 50. |
| `limit` | Hard pre-pagination cap. Max 10000. |

### Examples

```bash
localFindFiles(path=".", names=["*.test.ts"])
localFindFiles(path="packages", regex="^readme\\.md$")
localFindFiles(path=".", modifiedWithin="24h", entryType="f", details=true)
```

---

## `localGetFileContent`

Targeted file reading. Use it after structure/search has narrowed the file and section.

### Best For

- Reading a known line range.
- Extracting context around a known string or regex.
- Viewing a small whole file.
- Getting a structural skeleton without full bodies.

### Extraction Modes

Choose one main extraction mode:

| Mode | Fields |
|------|--------|
| Match extraction | `matchString`, optional `contextLines`, `matchStringIsRegex`, `matchStringCaseSensitive` |
| Line range | `startLine` and `endLine` |
| Whole file | `fullContent=true` |
| Structural skeleton | `minify:"symbols"` |

Do not combine `fullContent` with match or line-range extraction. Do not combine `matchString` with `startLine`/`endLine`.

### Key Parameters

| Parameter | Description |
|-----------|-------------|
| `path` | File path to read. Use `localViewStructure` for directories. |
| `startLine` / `endLine` | 1-based inclusive line range. Use together. |
| `matchString` | Anchor text or regex. |
| `contextLines` | Lines around each match. Default 5, max 100. |
| `matchStringIsRegex` | Treat `matchString` as regex. |
| `matchStringCaseSensitive` | Case-sensitive match search. |
| `charOffset` / `charLength` | Character pagination for large content. |
| `minify` | `symbols` for skeleton, `standard` for compact readable content, `none` for exact bytes. |
| `charOffset` | Continue character pagination when the response advertises more content. |

### Examples

```bash
localGetFileContent(path="packages/octocode-mcp/src/public.ts", startLine=1, endLine=80, minify="none")
localGetFileContent(path="README.md", matchString="Configuration", contextLines=4)
localGetFileContent(path="src/index.ts", minify="symbols")
```

---

## Local Workflows

### Explore A New Repository

```text
localViewStructure(path=root, recursive=true, maxDepth=1)
localViewStructure(path=root+"/src", recursive=true, maxDepth=2)
localFindFiles(path=root, names=["package.json", "tsconfig.json", "README.md"])
localSearchCode(path=root, keywords="export", filesOnly=true)
localGetFileContent(path="README.md", minify="symbols")
```

### Search Then Read

```text
localSearchCode(path="src", keywords="validateInput", contextLines=2)
localGetFileContent(path="src/validation.ts", matchString="validateInput", contextLines=20)
```

### Find Tests For A Feature

```text
localFindFiles(path=".", names=["*.test.ts", "*.spec.ts"])
localSearchCode(path="tests", keywords="featureName", filesOnly=true)
localGetFileContent(path="tests/feature.test.ts", matchString="featureName")
```

### Inspect Recent Changes

```text
localFindFiles(path=".", modifiedWithin="24h", entryType="f", details=true)
localSearchCode(path=".", keywords="TODO|FIXME", perlRegex=true)
```

---

## Rules

1. Use `localViewStructure` or `localFindFiles` before reading when the file is unknown.
2. Use `localSearchCode(filesOnly=true)` for fast discovery when match bodies are not needed.
3. Use `localSearchCode` with `contextLines` before opening a large file.
4. Use `localGetFileContent` with `matchString`, `startLine`/`endLine`, or `minify:"symbols"` instead of `fullContent` for large files.
5. Use pagination fields when a response advertises `hasMore=true`.

---

## Response Shape

- Bulk envelope: `results[]` with `data`, `hints`, `pagination`, `outputPagination`.
- `localViewStructure` returns structured `entries[]`, not a string.
- `localSearchCode` returns snippets, file-only lists, counts, or files-without-match per mode.
- `localGetFileContent` returns file slices only — not directory listings.

## Anti-Patterns

| Anti-Pattern | Better Approach |
|--------------|-----------------|
| `fullContent=true` on large files | Use `matchString`, line range, or `minify:"symbols"` |
| Search without scoping dirs | Use `excludeDir` to skip generated/vendor folders |
| Regex for exact literals | Use `fixedString=true` |
| Combining mutually exclusive flags | Pick one extraction mode |

**Parallelism:** independent queries run in parallel (batch limit: 5 per call). Sequential dependencies (`structure → search → read`) stay sequential.

---

## Related Documentation

- [Clone & Local Tools Workflow](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CLONE_WORKFLOW.md) - cloning repositories before local analysis.
- [GitHub Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/GITHUB_TOOLS.md) - remote GitHub search, fetch, clone, and PR tools.
- [Configuration Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CONFIGURATION.md) - environment variables and config file behavior.
