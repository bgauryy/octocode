# Locate answers in unread Chrome captures

Use this flow when semantic localization can replace broad host reads. For a literal, a tiny artifact, an already-read body, or a known small range, use direct search/read. Do not run the classifier after fetching the whole artifact into the host just to shorten it.

1. Capture once with this skill and retain artifacts under `.octocode/`. Before a private classification read, use `SNAPSHOT_STDOUT=summary node scripts/cdp-sandbox.mjs scripts/cdp-checks/page-snapshot.mjs --port <port> --keep-tab`: it prints counts/path while refs stay on disk. Default snapshot stdout lists refs; if those already settle the task, skip classification. Use the emitted artifact path; the path below is only a placeholder. Reuse the saved capture before fetching or opening a browser again.
2. Use metadata, titles, URLs and literal searches to narrow files without reading their bodies. Remove empty and duplicate captures.
3. Save the following request as `.octocode/clasify-request.json`. Replace the absolute path and atomic targets. Multiple files may share one matrix only when every question applies to every file; keep the expanded matrix within 25 cells.
4. Run the Octocode CLI command below. The same JSON works with MCP `clasify`; MCP returns `structuredContent` and mirrors it as JSON text.
5. Group nearby `best` / `answers.*.matches` windows into at most five exact read ranges per call. Read `source.path` with `localFetch` and verify the deciding source. Widen only when a sentence or record crosses a boundary; reuse that read across questions. For snapshot JSON, verify the complete object, including its reference key and label, before using a control. A line window can split that object.
6. Low `exists`, partial coverage, errors and conflicting evidence remain unresolved. Follow `next.clasify` unchanged when more relevant coverage is needed. Never turn a negative page judgment into whole-site absence.

```json
{
  "goal": "Find the page facts the next browser action depends on.",
  "reasoning": "Locate independent facts in an unread retained artifact before loading its body.",
  "resources": [{"id": "artifact", "context": {"tool": "localFetch", "query": {
    "goal": "Read the retained artifact for assessment",
    "reasoning": "Assess retained source without returning its body.",
    "path": "/absolute/path/to/.octocode/tmp/chrome-devtools/run/page-snapshot.json",
    "fullContent": true
  }}}],
  "questions": [
    {"id": "control", "questionType": "locate", "target": "The accessibility reference for the site search control."},
    {"id": "theme", "questionType": "locate", "target": "The accessibility reference for changing the page theme."}
  ]
}
```

```bash
octocode clasify --input .octocode/clasify-request.json
```

Inside the Octocode monorepo, use `node packages/octocode/out/octocode.js clasify --input .octocode/clasify-request.json`. With no installed executable, use `npx -y octocode clasify --input .octocode/clasify-request.json`. Inspect `scheme clasify --view query --compact` only when unfamiliar; do not reload it for each artifact.

Keep capture URL, timestamp and tab identity. Locate points into the saved artifact, not the live DOM. Before acting on an accessibility reference, confirm the control still exists in the current tab. Use live CDP evidence for changing state; a saved hint is not permission to act.

For logs, HAR or minified JSON with huge single lines, first use this skill’s bounded extraction/query tools to save the relevant records as readable text, preserving the original artifact and provenance. Do not interpret transformed text positions as original page/DOM coordinates. If the extraction is already small enough to answer, read it directly.

For uncertain candidates, a scoped `contribution` question can route the next read; hydrate search results only when discarded reads can outweigh preparation and verification. Do not add Scout → Judge stages automatically. Compare total host request, hint and verification tokens; fewer response bytes alone do not establish savings.

Next: verify and cite saved evidence using [intents-automation.md](intents-automation.md); stop when it answers the task.
