# Capture and cite browser evidence

Load when a browser answer needs evidence from saved captures: locate, classify, read and cite.

## Where evidence lives

Every call saves its full output under `<workspace>/.octocode/octocode-chrome-devtools/tmp/<run>/` (durable sessions: `.octocode/chromedevtools/<id>/`). The reply names the artifact `files`, the `evidence` manifest (which also lists the logs) and the `next.*` calls that page more. Typical files: `page-snapshot.json`, `page-nodes.txt`, `browser-result.json` (plan steps), `dom-step-N.json`, `live-network.har`, `network-bodies.json`, `console` and `stdout.txt` logs, screenshot manifests and tiles.

## Locate without reading

| Question | Command |
|---|---|
| Which file mentions X | Octocode `localSearch` on the reply's `files`/`evidence` paths with `includeHidden:true, includeIgnored:true, defaultExcludes:false` (`matchString`, `filesOnly`; the folder is hidden and git-ignored); shell: `rg -F -l 'X' .octocode/octocode-chrome-devtools/tmp` |
| Failed requests in a HAR | `helper` `{name: "har-pager", args: ["<file>","--filter","failures"]}` (full preset or `/cli`), or `jq '.log.entries[] \| select(.response.status>=400) \| {url:.request.url,status:.response.status}' file.har` |
| One field of a JSON capture | `query` with `pointer` and `where`, or `jq` |
| Text around a hit | `localFetch` with the hit's line range, or `rg -n -C3 'X' file` |
| Control refs on the page | the inline `snapshot` text; else `helper` `snapshot-query` / `localSearch` over `page-nodes.txt` |

Search narrow first (one run folder, then the tmp tree). A search that found nothing proves only the folders and spellings it covered.

## Classify before reading (when `clasify` is enabled)

When many files, long logs, scripts, HAR bodies or screenshots could hold the answer, call Octocode `clasify` instead of reading them: pass the saved paths (or a `localSearch` result set) with independent questions about stated or visible facts ("does this response set a session cookie?", "which console lines are errors from our code?"). It returns which items matter; read only those (`next.read`) and cite after reading. If `clasify` is not enabled (no classification key configured), use `localSearch` with a precise `matchString` and read ranges.

Classification gives candidates, never proof. Verify accessibility, causes, exact relations and counts structurally (refs, statuses, byte counts). Before acting on something found in a classified image or saved ref, confirm live actionability and permission.

## Read, verify and cite

Read the deciding span, follow continuations unchanged and resolve any unread scope that could change the answer. Cite each claim beside its source URL or capture path (and line, request id or node ref); name unread scope or limits. [Output and saved reports](../output.md) covers findings, steps and gaps.
