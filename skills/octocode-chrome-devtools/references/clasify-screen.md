# Capture resources for an explicit classification request

`octocode-clasify` owns admission, questions, coverage, and verification. Use its gate before this recipe; ordinary investigation searches and reads source directly. This file only maps saved artifacts to resources.

- Search metadata and exact text first using `scripts/corpus-run-local.mjs` or the relevant check output; omit empty and duplicate artifacts. Use direct search when a literal can express the target.
- For semantic targets over unread saved text, supply `localFetch` resources with absolute paths and one atomic `questionType:"locate"` target per question. It returns one small ranked verification window plus `exists`, so the host can inspect only that source area. For already observed values, use `context.value`.
- Keep each resource ID linked to its original artifact and URL metadata. Apply the caller’s explicit questions without adding a relevance or routing bundle.
- After assessment, inspect the deciding original spans before making a factual claim. Source coordinates and transformed-view coordinates remain distinct.

Next: query the retained capture; bridge HAR-derived bodies with `scripts/har-ingest-to-scrape.mjs` only when the optional scraping skill helps the task.

## Executable CLI bridge

Run `octocode scheme clasify --view query --compact` once when the contract is unfamiliar. After `page-snapshot.mjs` emits its artifact, classify the unread file through Octocode CLI; do not paste the snapshot into `context.value`. This shape was verified against a live TypeSafe page snapshot:

```bash
SNAPSHOT="$PWD/.octocode/tmp/chrome-devtools/<run>/page-snapshot.json"
octocode clasify "{\"id\":\"chrome-snapshot-locate\",\"reasoning\":\"Locate a requested control before reading the retained browser snapshot.\",\"resources\":[{\"id\":\"page-snapshot\",\"context\":{\"tool\":\"localFetch\",\"query\":{\"reasoning\":\"Assess the retained accessibility snapshot without returning its body.\",\"path\":\"$SNAPSHOT\",\"fullContent\":true}}}],\"questions\":[{\"id\":\"search-control\",\"question\":{\"questionType\":\"locate\",\"target\":\"Which accessibility reference opens the site search control?\"}}]}"
```

Read `source.path` at `matches[0].startLine/endLine` before acting on a ref. The returned window is deliberately small; widen by a few adjacent lines if a JSON record crosses it. Low `exists`, partial coverage, or an error remains unresolved.
