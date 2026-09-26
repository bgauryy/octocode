# Locate answers in unread scrape text

Use this flow when semantic localization can replace broad host reads. For a literal, a tiny artifact, an already-read body, or a known small range, use direct search/read. Do not run the classifier after fetching the whole artifact into the host just to shorten it.

1. Capture once with this skill and retain artifacts under `.octocode/`. Use the emitted artifact path; the path below is only a placeholder. Reuse the saved capture before fetching or opening a browser again.
2. Use metadata, titles, URLs and literal searches to narrow files without reading their bodies. Remove empty and duplicate captures.
3. Save the following request as `.octocode/clasify-request.json`. Replace the absolute path and atomic targets. Multiple files may share one matrix only when every question applies to every file; keep the expanded matrix within 25 cells.
4. Run the Octocode CLI command below. The same JSON works with MCP `clasify`; the host must consume `structuredContent`, because the text `content` array is empty.
5. Group nearby `answers.*.matches[0]` windows into at most five exact read ranges per call. Read `source.path` with `localFetch` and verify the deciding source. Widen only when a sentence or record crosses a boundary; reuse that read across questions.
6. Low `exists`, partial coverage, errors and conflicting evidence remain unresolved. Follow `next.clasify` unchanged when more relevant coverage is needed. Never turn a negative page judgment into whole-site absence.

```json
{
  "reasoning": "Locate independent facts in an unread retained artifact before loading its body.",
  "resources": [{"id": "artifact", "context": {"tool": "localFetch", "query": {
    "reasoning": "Assess retained source without returning its body.",
    "path": "/absolute/path/to/.octocode/tmp/scrape/session/text/page.clean.md",
    "fullContent": true
  }}}],
  "questions": [
    {"id": "choice", "questionType": "locate", "target": "What the Choice primitive returns."},
    {"id": "score", "questionType": "locate", "target": "What the Score primitive returns."}
  ]
}
```

```bash
octocode clasify --input .octocode/clasify-request.json
```

Inside the Octocode monorepo, use `node packages/octocode/out/octocode.js clasify --input .octocode/clasify-request.json`. With no installed executable, use `npx -y octocode clasify --input .octocode/clasify-request.json`. Inspect `scheme clasify --view query --compact` only when unfamiliar; do not reload it for each artifact.

Keep the clean-text artifact linked to its URL and extraction metadata. Cite the original URL and the verified saved source.

For logs, HAR or minified JSON with huge single lines, first use this skill’s bounded extraction/query tools to save the relevant records as readable text, preserving the original artifact and provenance. Do not interpret transformed text positions as original page/DOM coordinates. If the extraction is already small enough to answer, read it directly.

For uncertain candidates, a scoped `contribution` question can route the next read; hydrate search results only when discarded reads can outweigh preparation and verification. Do not add Scout → Judge stages automatically. Compare total host request, hint and verification tokens; fewer response bytes alone do not establish savings.

Next: verify and cite saved evidence using [extraction-quality.md](extraction-quality.md); stop when it answers the task.
