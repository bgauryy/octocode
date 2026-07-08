<search_and_research>
Plan scope before searching. Never guess tool fields or line numbers.

**Minify** (always, unless exact bytes needed)
- `symbols` — orient large files >200 lines; preserves line anchors for LSP
- `standard` — configs, data, non-code
- `none` — edits, diffs, exact match, citations

**Workflow**
1. **Structure** — `localViewStructure` / `ghViewRepoStructure`; orient before reading code; use `symbols`/AST for code and minified docs outlines before body text.
2. **Search** — `localSearchCode` / `ghSearchCode`; broad → narrow by path / language / symbol / literal.
3. **Fetch** — use `localGetFileContent`/`ghGetFileContent` only for known targets: `matchString`, lines, or symbols; prefer minified content; whole files only when needed.
4. **Prove** — use AST for shape and `lspGetSemantics` for definitions/references/callers/types; loop back if evidence changes.

Nav: `symbols`/AST → anchor → `matchString`/range `none` → LSP `lineHint`.
`lineHint` MUST come from search results, `matchRanges`, AST captures, or document symbols — never guessed.

**Research loop** — after every result ask: What changed? Is the answer good enough? Stop when more tools would not change the decision.
- Snippets are leads, not proof. Confidence: `confirmed` (two sources or one deterministic check) · `likely` (one source) · `uncertain` (hypothesis/snippet).
- `empty` = ran, matched nothing → change one variable (query, path, filter, surface) before treating as absence.
- `error` = broken call (auth, validation, rate limit) → fix the call; never read it as absence.
- Carry anchors exactly: `paths` · `lines` · `matchRanges` · `next.*` · `charOffset` — never invent or calculate.
- Lightest proof first: search → exact read → AST shape → LSP identity → independent corroboration.

**Flows by kind**
- local code → `localSearchCode` (`structural` for AST shape) → confirm with `lspGetSemantics`; prefer LSP identity over raw file reads.
- docs → search/outline first → fetch the relevant section with minify; avoid full-document reads unless exact bytes or global context is required.
- external/ecosystem → `ghSearchCode` / `ghGetFileContent` / `ghViewRepoStructure` / `npmSearch` / `web`.
- dependency → inspect `node_modules/<pkg>/` source directly before inferring from docs or types.
- npm → `npmSearch` → `ghGetFileContent` / `ghViewRepoStructure`
- local → verify upstream: `localSearchCode` → `ghSearchCode` / `ghGetFileContent`
- GitHub finding → validate locally: `ghGetFileContent` → `localSearchCode` / `lspGetSemantics`

Ask before: broad public-contract changes, destructive actions, cloning many repos, untrusted execution.
Reviews: lead with severity; each finding needs `file:line`, impact, proof, confidence, smallest safe fix.
</search_and_research>
