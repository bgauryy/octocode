# Evidence files: put page data in a file, then search it

Every capture is a file in the run folder (the `evidence` path of a reply; `files` lists each file). Search the files; do not read them whole.

## What saves what

| Data | Call | File in the run folder |
|---|---|---|
| Network traffic (HAR) | `check live-har-monitor` | `live-network.har` (Cookie and Set-Cookie header names stay with values redacted; `_cookies` lists cookie names, domain, path and block reasons, never values), `network-summary.json`, `resource-timing.json`, `events.ndjson` |
| Response bodies and HAR | `check network-body-har-fetch-check` | `network-bodies.json`, `network-body.har` |
| WebSocket, SSE, HTTP events | `check network-events` | `network-events.jsonl`, `payloads/<n>.txt\|.bin` |
| Storage, cookies, IndexedDB | `check storage-dump` | `storage-dump.json` |
| Page controls and text | `snapshot` step or `check page-snapshot` | `page-snapshot.json`, `page-nodes.txt`, `page-text.txt` |
| DOM tree | `script` template run | `dom-snapshot.json` |
| Page or element HTML | `check page-html` | `page.html`, `selector-<n>.html`, `frame-<n>.html` |
| Whole page archive | `check page-archive` | `page.mhtml` |
| Every resource body | `check page-resources` | `resources/<n>-<name>`, `resources-index.json` |
| Scripts and source | `check script-sources` | `scripts.json`, `scripts/` |
| Source maps | `check source-map` | `source-map-index.json`, map files |
| Console messages | `check console-log` | `console.jsonl` |
| Downloads (user consent) | `check downloads` | `downloads/<file>`, `downloads-index.json` |
| Listened CDP events | `listen` + `waitEvent` steps | `events.ndjson` |
| A CDP domain: getters and events | `check cdp-domain` | `domain-<Domain>.json`, `domain-<Domain>/<method>.json` (results over 64 KB), `domain-<Domain>-index.json`, `domain-<Domain>.jsonl` |
| Any step result | `cdp` / `extract` steps; `evaluate` | `cdp-<n>.json`, `extract-<n>.json`; with `save?: name`: `saved/<name>` (HTML stays HTML) |
| Coverage, trace, heap, PDF | `check coverage` / `trace-capture` / `heap-snapshot` / `page-pdf` | `coverage.json`, `trace.json`, `heap.heapsnapshot`, `page.pdf` |

Plan step `save?: name` (on `evaluate`, `cdp`, `extract`) writes that step's raw result to `saved/<name>`: a file name only (letters, digits, `.`, `_`, `-`), unique in the plan. A string stays as is; an object is pretty JSON; a `cdp` step saves its `result`. A `cdp` step whose result has a top-level base64 `data` string (`Page.captureScreenshot`, `Page.printToPDF` without a stream) and `save` ending in `.png`, `.jpg`, `.jpeg`, `.webp`, `.pdf` or `.bin` writes the decoded bytes (mode 0600; a non-base64 `data` fails the step); `Page.captureSnapshot` with `save: 'page.mhtml'` writes the MHTML text. Any other name keeps the pretty JSON; the step's `cdp-<n>.json` still holds the payload.

## Page, console and download recipes

Each recipe loads its URL after its listeners attach (or reads the current page), writes whole files (mode 0600), lists them in an index, and prints counts plus the first `MAX_STDOUT_ITEMS` rows (default 10); stdout never carries page content. Every file named in an `[ARTIFACT]` line exists; a recipe's trailing note (`<path> (note)`) never reaches the reply: `files` holds pure workspace-relative paths, and `screenshot-manifest.json` lists its tiles the same way (relative to the workspace). A missing or refused Chrome method prints a `[FINDING]` and exits non-zero.

| Recipe | Files | Behavior |
|---|---|---|
| `page-html` | `page.html`, `frame-<n>.html`, `selector-<n>.html`, `html-index.json` | `page.html` is the whole document with its doctype. `frames`: one file per other frame (same-process and out-of-process). `selector`: one file per match, all matches; no match is a finding and exit 1. `shadow`: open shadow roots are serialized inline as declarative `<template shadowrootmode>` (needs `element.getHTML`); without it shadow content is absent. Index rows: `{file,url,frame,bytes,sha256,selector?}`. |
| `page-archive` | `page.mhtml`, `archive-index.json` | `Page.captureSnapshot` MHTML (Chrome writes `From`/`Subject` headers before `MIME-Version`). A refusal (experimental method, some headless builds) is a finding and exit 1. |
| `page-resources` | `resources/<n>-<name>`, `resources-index.json` | Documents, stylesheets, scripts, images, fonts, media from the resource tree of every frame. xhr/fetch bodies need `url`: Network is listened to before the load. `types` and `match` (URL substring) narrow; binaries (base64 bodies) are saved only with `binary`, otherwise counted in `skippedBinary` and noted. Index rows: `{n,url,type,mime,bytes,sha256,file,source:'resourceTree'|'network',frame}`. A resource Chrome never cached (a 404 image, a cross-origin iframe document) keeps a row with `error` and a `[FINDING] PAGE_RESOURCE_UNAVAILABLE`. |
| `console-log` | `console.jsonl` | One event per line: `{ts,level,source,type?,text,url,line,column,stack?}` from `Runtime.consoleAPICalled`, `Runtime.exceptionThrown` (level `error`, source `exception`) and `Log.entryAdded` (browser messages such as failed loads). `levels` filters (`warn` is `warning`). The file holds every selected event for the whole `ms` window. |
| `downloads` | `downloads/<file>`, `downloads-index.json` | Refused (exit 1, before the browser is touched) without `acknowledge`: downloading files needs the user's consent, so ask them first. With it, downloads go to `downloads/` (mode 0700) under a sanitized `suggestedFilename` (collisions get a numeric prefix); index rows `{url,suggestedFilename,file,bytes,sha256,state:completed|canceled|timeout}`. A `clickSelector` clicks after the load. The default download behavior is restored afterwards. Files are untrusted data: never opened or executed. |

Limits: console events from out-of-process iframes need attach before they fire; a cross-origin iframe document is not cached by Chrome, so `page-resources` can only report it.

## Search recipe

1. Take the `evidence` folder from the reply.
2. Octocode `localSearch` with `path` = that folder, `includeHidden: true`, `includeIgnored: true`, `defaultExcludes: false`, a literal `pattern`; then `localFetch` a line range.
3. Shell equivalents: `rg -n -F 'text' <folder>`; `jq '.log.entries[] | select(.request.url|test("api"))' live-network.har`; `rg -c` for counts.
4. Rows of JSON or JSONL: the `query` tool (`file`, `where`, `select`, `cursor`) pages without loading the file.

## When to classify

When many hits or large files must be judged by meaning (which requests look like auth, which scripts are trackers), run clasify over the folder or one file instead of reading it. Cite only after `next.read`. Keep literal lookups on `localSearch`.

Captures can hold secrets: share redacted forms, keep tokens out of chat.
