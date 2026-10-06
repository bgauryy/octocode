# Locate answers in unread scrape text

Load when an unread saved artifact needs semantic location and a small direct read does not decide.

1. Capture once with this skill and retain artifacts under `.octocode/`. Use the emitted artifact path; the path below is only a placeholder. Reuse the saved capture before fetching or opening a browser again.
2. Use metadata, titles, URLs and literal searches to narrow files without reading their bodies. Remove empty and duplicate captures.
3. Save the following request as `.octocode/clasify-request.json`. Replace the absolute path and atomic targets. Multiple files may share one matrix only when every question applies to every file; keep the expanded matrix within 25 cells.
4. Run the Octocode CLI command below. The same JSON works with MCP `clasify`.
5. Group nearby `best` / `answers.*.matches` windows into at most five exact read ranges per call. Read `source.path` with `localFetch` and verify the deciding source. Widen only when a sentence or record crosses a boundary; reuse that read across questions.
6. Follow the `next.clasify` page unchanged when more relevant coverage is needed. Never turn a negative page judgment into whole-site absence.

```json
{"queries": [{
  "mainGoal": "Find what the Choice and Score primitives return in the saved page.",
  "reasoning": "Locate independent facts in an unread retained artifact before loading its body.",
  "resources": [{"id": "artifact", "tool": "localFetch", "query": {
    "path": "/absolute/path/to/.octocode/tmp/scrape/session/text/page.clean.md",
    "fullContent": true
  }}],
  "questions": [
    {"id": "choice", "type": "locate", "ask": "What the Choice primitive returns."},
    {"id": "score", "type": "locate", "ask": "What the Score primitive returns."}
  ]
}]}
```

```bash
octocode clasify --input .octocode/clasify-request.json
```

Inside the Octocode monorepo, use `node packages/octocode/out/octocode.js clasify --input .octocode/clasify-request.json`. With no installed executable, use `npx -y octocode clasify --input .octocode/clasify-request.json`.

Keep the clean-text artifact linked to its URL and extraction metadata. Cite the original URL and the verified saved source.

For logs, HAR or minified JSON with huge single lines, first use this skill’s bounded extraction/query tools to save the relevant records as readable text, preserving the original artifact and provenance. Do not interpret transformed text positions as original page/DOM coordinates. If the extraction is already small enough to answer, read it directly.

Next: verify and cite saved evidence using `references/data-contract.md` § Extraction quality; stop when it answers the task.
