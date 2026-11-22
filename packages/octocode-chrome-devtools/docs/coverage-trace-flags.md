# Capture recipes and launch flags

In the typed CLI, run `/cli check --recipe <name> --options '<json>'`. Each recipe returns paths to files in the run folder.

| Recipe | Options | Files | Stdout |
|---|---|---|---|
| `coverage` | `COVERAGE_URL`, `COVERAGE_MS`, `COVERAGE_JS`, `COVERAGE_CSS` (both default on) | `coverage.json`, `coverage-ranges.json` | per-type totals, top 20 scripts or sheets by unused bytes, `[PAGE]` line |
| `trace-capture` | `TRACE_MS` (required), `TRACE_URL`, `TRACE_CATEGORIES` (JSON array) | `trace.json` | bytes, chunks, data-loss flag |
| `heap-snapshot` | `HEAP_COLLECT_GARBAGE` | `heap.heapsnapshot` | byte size |
| `page-pdf` | `PDF_LANDSCAPE`, `PDF_PRINT_BACKGROUND`, `PDF_SCALE` (0.1 to 2) | `page.pdf` | byte size, transfer mode |

## coverage

JS uses `Profiler.startPreciseCoverage` (`callCount`, `detailed`); CSS uses rule usage tracking.
Coverage starts before `COVERAGE_URL` loads, then waits `COVERAGE_MS` (an interaction window).
Without a URL it measures the current page from now.

- `coverage.json`: `entries[]` of `{url, type, id, totalBytes, usedBytes, unusedBytes, percentUsed}`, one per script or
  stylesheet, plus `totals`. `usedBytes + unusedBytes == totalBytes` for every entry.
- `coverage-ranges.json`: `ranges[]` of `{url, type, id, start, end}`, every unused range. Offsets are UTF-16 code
  units of the script or stylesheet text. No set is capped.
- Nested block coverage is resolved innermost first: a never-called function inside a called one is unused.
- CSS: Chrome reports only the rules that applied; every other byte of the sheet counts as unused (as in the DevTools
  Coverage tab), so comments and at-rule text count as unused.
- Scripts without a URL (code evaluated by tools) are skipped; the count is `skippedAnonymousScripts` and a finding line.

## trace-capture

`Tracing.start` with `transferMode: ReturnAsStream`, waits `TRACE_MS` (loading `TRACE_URL` after the start), then
`Tracing.end` and reads the stream to `trace.json` chunk by chunk. The file is Chrome trace JSON
(`{"traceEvents": [...]}`) for Perfetto or the DevTools Performance panel. Default categories: `devtools.timeline`,
`disabled-by-default-devtools.timeline`, `v8.execute`, `blink.user_timing`, `loading`, `latencyInfo`.

## heap-snapshot

Optional `HeapProfiler.collectGarbage`, then `takeHeapSnapshot`; every `addHeapSnapshotChunk` is written to disk in
arrival order, so the snapshot is never held in memory. Load the file in the DevTools Memory panel.

## page-pdf

`Page.printToPDF` with `transferMode: ReturnAsStream`, falling back to the base64 reply when Chrome refuses it.

## Typed `open.launch` flags

These fields sit inside the one `launch` object of the typed `open` input (`{"launch": {"windowSize": "1280x800"}}`); the engine flags below are what they become.

| Field | Engine flag | Effect |
|---|---|---|
| `enableFeatures` | `--enableFeatures` | one `--enable-features` (WebMCP always included) |
| `disableFeatures` | `--disableFeatures` | one `--disable-features` |
| `blinkFeatures` | `--blinkFeatures` | one `--enable-blink-features` |
| `chromeFlags` | `--chrome-flags a,b` | appended after the managed flags |
| `windowSize`, `userAgent`, `chromePath`, `proxy` | existing | as before |

- Each `chromeFlags` entry is `--name` or `--name=value` and is checked against the contract's `chromeFlagDenylist`
  by the engine, so a direct call is refused too. The error names the flag and the reason. A flag the launch already
  sets (for example `--no-first-run`) is refused; one name given twice with different values is refused.
- The reply lists `appliedFlags` (every extra flag passed). A proxy shows only as `proxy: true`, never its server.
- A port that already has a browser keeps its launch flags: the reply says `launchFlagsIgnored: true`.
- `--attach` with any launch flag is refused.
