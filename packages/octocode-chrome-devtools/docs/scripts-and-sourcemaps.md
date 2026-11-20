# Scripts and source maps

Two `check` recipes read what a page runs. Both save evidence to files in the run folder; stdout carries an index only.

## `script-sources`

`check --recipe script-sources` enables the Debugger before it loads anything, so every `scriptParsed` event is seen. Without `SCRIPTS_URL` or `SCRIPTS_RELOAD` it reads the current page (Chrome replays the scripts that already exist).

| Option | Effect |
|---|---|
| `SCRIPTS_URL` | Navigate to this URL after listeners attach |
| `SCRIPTS_RELOAD` | Reload the current page (cache ignored) after listeners attach |
| `SCRIPTS_MATCH` | Keep scripts whose URL contains this text |
| `SCRIPTS_INLINE` | Also keep inline and eval scripts (no URL, or the document's own URL) |

Output in the run folder:

- `scripts.json`: one row per script: `n`, `scriptId`, `url`, `length` (UTF-16 units), `hash`, `sourceMapURL`, `isModule`, `hasSourceURL`, `startLine`, `endLine`, `executionContextId`, `inline`, plus `file`, `bytes` (UTF-8) and `sha256` of the saved source.
- `scripts/<n>-<hash>.js`: the full source from `Debugger.getScriptSource`. Never truncated; a large file is whole and its size is in `bytes`.

Stdout: `[METRIC] SCRIPTS`, up to `MAX_STDOUT_ITEMS` (default 10) lines `[SCRIPT] n url bytes sourceMap=yes|no`, the `[ARTIFACT]`/`[NEXT]` lines that query `scripts.json` for the rest, and a `[FINDING]` for a script whose source could not be read. Source text is never printed.

## `source-map`

`check --recipe source-map` finds one script (`SOURCEMAP_SCRIPT`: scriptId, exact URL, or a unique URL substring), reads its `sourceMapURL`, and resolves the map.

| Option | Effect |
|---|---|
| `SOURCEMAP_SCRIPT` | Required. Script url or scriptId |
| `SOURCEMAP_POSITIONS` | `line:col,line:col` generated positions to map |
| `SOURCEMAP_LIST_SOURCES` | Print one `[SOURCE]` line per original source |
| `SOURCEMAP_SAVE_SOURCES` | Write each original source from `sourcesContent` under `sources/` |

Position convention: generated line is 1-based and column is 0-based (Chrome's column). The original `line` is 1-based and `column` 0-based. A position maps to the nearest mapping at or before its column on the same line.

Map access:

- `data:` URIs (base64 or percent-encoded) decode locally.
- `http(s)` maps are fetched through the sandbox's narrow resolver exemption, only when the map shares the script's origin. A different origin is refused (`SOURCEMAP_REFUSED`); network access is not widened.

Output: `<script name>.map.json` (the map as served, `sourcesContent` kept), `source-map-index.json` (sources with `hasContent`, `savedAs`, and mapped positions), `[POSITION] 2:3 -> src/b.ts:6:2 name`, and `[SOURCE]` lines when listed.

Saved sources: each `sources` entry is reduced to a relative path (scheme, drive letters, empty, `.` and `..` segments dropped; unusual characters become `_`) and confined to `sources/`, so a source like `../../etc/passwd` lands at `sources/etc/passwd`.

Findings and exit codes:

| Finding | Meaning | Exit |
|---|---|---|
| `SOURCEMAP_MISSING` | Script has no `sourceMappingURL` | 0 |
| `SOURCEMAP_POSITION_UNMAPPED` | No mapping on that generated line at or before the column | 0 |
| `SOURCEMAP_MALFORMED` / `SOURCEMAP_UNAVAILABLE` | Map is invalid, indexed (`sections`), or could not be fetched | 1 |
| `SOURCEMAP_REFUSED` | Foreign-origin or unsupported-scheme map | 1 |
| `SOURCEMAP_SCRIPT_NOT_FOUND` / `_AMBIGUOUS` | No script, or several URLs match | 1 |
| `SOURCEMAP_POSITION_INVALID` | A position is not `line:col` with line >= 1 | 1 |
