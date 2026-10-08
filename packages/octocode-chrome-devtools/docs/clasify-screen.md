# Locate answers in unread captures

Load when a saved artifact is unread and a literal search or small read does not decide; admission and result rules belong to the `octocode-research` clasify gate. Why: Locate deciding evidence without loading an entire capture.

1. Capture once. `SNAPSHOT_STDOUT=summary` keeps snapshot refs on disk; if the default stdout refs already answer, stop.
2. Narrow with metadata, URLs, and literal search. For HAR, huge logs, or minified JSON, first extract relevant records to readable text with `har-pager`/`measure-query`; keep the original for provenance.
3. Save `.octocode/clasify-request.json` (all questions must apply to every resource; at most 25 cells):

```json
{"queries": [{
  "mainGoal": "Find the page facts the next browser action depends on.",
  "reasoning": "Locate facts in an unread retained artifact before loading it.",
  "resources": [{"id": "artifact", "tool": "localFetch", "query": {
    "path": "/abs/.octocode/tmp/chrome-devtools/<run>/page-snapshot.json", "fullContent": true
  }}],
  "questions": [
    {"id": "search", "type": "locate", "ask": "The accessibility ref for the site search control."}
  ]
}]}
```

4. Run `octocode clasify --input .octocode/clasify-request.json` (monorepo: `node packages/octocode/out/octocode.js`; else `npx -y octocode`). MCP `clasify` takes the same JSON.
5. Merge nearby windows into at most five `localFetch` ranges and verify the deciding source. For snapshot JSON, read the whole ref object.
6. Follow the `next.clasify` page unchanged. A negative page result is not site-wide absence.

Hints point into the saved capture, not the live DOM: confirm a ref still exists in the current tab before acting.

Next: act on a confirmed ref with `dom-operations-check` (`cdp-checks.md`).
