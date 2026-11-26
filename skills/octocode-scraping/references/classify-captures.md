Load when captured text is too large to read and an evidence question must choose the passages: narrowing captures and calling `clasify`.

# Classify captures

## Find evidence in unread text (classifiers, clasify)

- Admission and result rules belong to `octocode-research`; inspect the tool's live schema (`octocode schema clasify`) before calling it.
- Reuse the saved capture; narrow files by metadata, titles, URLs, and literal search. Skip empty and repeated captures by data: `node <skill-folder>/scripts/corpus-inspect.mjs --session-dir <d> --view captures` lists each capture's `bytes` and `sha256`; drop rows marked `empty` or carrying `duplicateOf` (`captureSummary` counts both).
- Log, HAR, or minified JSON: extract records as text first, keeping the original (positions are not DOM coordinates); read small ones directly.
- Which passages to read: `localSearch` over the capture directory with a literal; when an evidence question can choose the next passages to read, run that search as `clasify` `source:{tool:"localSearch",query:{...}}` with `predicates:[...]`.
- A typed judgment about one capture file: `clasify` with a `source` reference; save large requests as `.octocode/tmp/octocode-scraping/clasify-request.json`.

```json
{"queries": [{
  "operation": "evaluate",
  "source": {"tool": "localFetch", "query": {
    "path": "/abs/.octocode/octocode-scraping/tmp/<sessionId>/text/<pageId>.clean.part-001.md"
  }},
  "questions": {
    "choice_return": {"type": "noul", "instructions": "Does the page state what a Choice question returns?"},
    "score_return": {"type": "noul", "instructions": "Does the page state what a Score question returns?"}
  }
}]}
```

- Run: `npx -y octocode clasify --input .octocode/tmp/octocode-scraping/clasify-request.json` (MCP `clasify` takes the same JSON when available). A capture over the state limit fails its row: select `ranges`.
- Before citing, follow the emitted captured-source continuation or read the deciding lines with `localFetch`; `hints` are prose, not executable reads. A low answer never proves whole-site absence.
