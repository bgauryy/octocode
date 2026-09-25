# Optional corpus relevance screen

Load when paths, URL metadata, titles, snippets, and a cheap literal search leave several unread pages or parts ambiguous, and a semantic result would change which one you read next. This is the owner of corpus Scout routing. Skip the call when visible evidence identifies the needed page or a direct read is cheaper. A page count or size alone is not a trigger.

## Prepare

- Drop empty, thin, and duplicate extractions using metadata. Keep known required pages in the read set.
- Ask one concrete relevance question for the research direction. A page is relevant if it contributes even one useful fact, constraint, counterexample, or lead; it need not answer the whole question. Implementation wording belongs only to implementation lookup.
- Submit ambiguous unread pages as `localFetch` `{tool,query}` resources with absolute paths. Use a heading-bounded section or targeted line range when known. Preserve the complete meaning of that section; an arbitrary `maxChars` cap can silently leave coverage partial. Split genuinely large documents by meaningful sections and keep `resources × questions ≤ 25` per matrix.
- Add a content-type or route Choice only when its answer changes the next action. The runtime adds `insufficient` to Choice. Use root `queries[]` only for independent matrices.

Call `octocode clasify --input <request>.json` directly when configured. Exit 6 requires the unchanged `next.clasify` continuation. Read page scopes and coverage in `queries[].resources[].pages[]`; map each resource ID to its absolute local path for the exact follow-up read. `partial`, errors, and uncertainty remain open. Scores help order reads and do not impose a universal exclusion threshold or prove absence. Read the smallest deciding source span, then verify the actual fact with `scripts/corpus-run.mjs --session-dir <d> --roots text --regex <term>` or a direct corpus read. Zero literal matches can mean different wording or absent text; inspect the page and rendering evidence before escalating to Chrome.

When clasify is unavailable or unhelpful, use `scripts/corpus-find.mjs` for page ranking and `scripts/corpus-run.mjs` for exact line locations. Cite inspected source text, never a semantic route. Next: retrieve the selected span with `scripts/corpus-inspect.mjs` or `scripts/corpus-find.mjs`, then cite per `references/extraction-quality.md`.
