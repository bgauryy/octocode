# Corpus resources for an explicit classification request

`octocode-clasify` owns admission, questions, coverage, and verification. Use its gate before this recipe; ordinary investigation searches and reads source directly. This file only maps saved artifacts to resources.

- Search metadata and exact text first using `scripts/corpus-find.mjs` and `scripts/corpus-run.mjs`; omit empty and duplicate artifacts. Use direct search when a literal can express the target.
- For semantic targets over unread saved text, supply `localFetch` resources with absolute paths and one atomic `questionType:"locate"` target per question. It returns one small ranked verification window plus `exists`, so the host can inspect only that source area. For already observed values, use `context.value`.
- Keep each resource ID linked to its original artifact and URL metadata. Apply the caller’s explicit questions without adding a relevance or routing bundle.
- After assessment, inspect the deciding original spans before making a factual claim. Source coordinates and transformed-view coordinates remain distinct.

Next: inspect the selected page with `scripts/corpus-inspect.mjs`, then cite per `references/extraction-quality.md`.

## Executable CLI bridge

Run `octocode scheme clasify --view query --compact` once when the contract is unfamiliar. After `fetch.mjs` creates clean text, point Clasify at that file; never paste the body into the command. This two-question matrix was verified against a retained TypeSafe introduction page and captured the file once:

```bash
ARTIFACT="$PWD/.octocode/tmp/scrape/<session>/text/page-001.clean.part-001.md"
octocode clasify "{\"id\":\"scrape-locate\",\"reasoning\":\"Locate two answers in retained scrape text before reading the artifact.\",\"resources\":[{\"id\":\"saved-page\",\"context\":{\"tool\":\"localFetch\",\"query\":{\"reasoning\":\"Assess retained clean text without returning its body.\",\"path\":\"$ARTIFACT\",\"fullContent\":true}}}],\"questions\":[{\"id\":\"choice-output\",\"question\":{\"questionType\":\"locate\",\"target\":\"What does the Choice primitive return?\"}},{\"id\":\"score-output\",\"question\":{\"questionType\":\"locate\",\"target\":\"What does the Score primitive return?\"}}]}"
```

Read `source.path` at each `matches[0].startLine/endLine`; widen by a few adjacent lines only when a sentence or structured record crosses the returned window. Low `exists`, partial coverage, or an error remains unresolved.
