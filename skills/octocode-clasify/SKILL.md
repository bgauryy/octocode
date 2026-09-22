---
name: octocode-clasify
description: "Judge supplied context, or screen unread candidates/large resources if reading would consume model context and bounded judgment can choose what to inspect. Not for exact facts, deterministic checks, proof, global absence, or free-form summaries. Returns body-free verdicts with confidence, coverage, and page scopes. Next: act, or read deciding scopes with the exact tool; verify exact/absence claims."
---
# Semantic assessment

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research` · `octocode-scraping` · `octocode-chrome-devtools`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Flow: `INSPECT → SHAPE → ASSESS → VERIFY`.

**Use when** bounded judgment over supplied context or unread resources can change the next action — especially when reading all candidates would consume model context, or one large resource needs region selection before reading. Pass unread resources as delegated read queries (not fetched bodies). Use supplied state to judge evidence already held, synthesize, or self-review.

**Not for:** exact facts (use grep/localFetch/lspSearch); deterministic checks (version match, syntax valid, auth test, path exists); global absence claims; free-form summaries. Never assert a claim or write a report/audit finding from a score — confirm on fetched bytes first. Use `corpus-run --regex` for literal text presence; `localSearch`/`astSearch`/`lspSearch` for structural or symbol evidence.

**Invoked from other tools** (authoritative “clasify when:” patterns):
- `ghSearch` — many unread repos, paths, or code hits precede a pick
- `ghGetFileContent` / `localFetch` — unread files compete for a read, or a large file’s region must be found first
- `ghSearchHistory` / `ghGetHistoryItem` — unread PRs, issues, or commits need triage
- `artifactSearch` — several candidate packages need relevance triage
- `ghCloneRepo` — screening repos before a costly clone
- `localSearch` — many matched files need triage before reads
- `astSearch` — many candidate files, matches, or topology edges need triage
- `lspSearch` — unread reference sites need triage
- `octocode-scraping` SCREEN step — triage corpus pages; route to `read`/`consider`/`skip`/`cdp-needed`
- `octocode-chrome-devtools` SCREEN step — triage DOM snapshots, HAR bodies, and network summaries
- `octocode-research` — file triage when many candidates compete
- Any agent — self-review: context.value holds a draft plus rationale, evidence, and assumptions

**Question types:** Noul = P(yes) for one proposition · Choice = one named class or `insufficient` · Score = one ordered dimension (2–10 described levels). Several aspects = several questions; never hide a checklist inside one instruction.

**Matrix rules:**
- `resources[] × questions[]` when every question applies to every resource (one rubric)
- root `queries[]` only for independent matrices whose cross-product would be wrong
- Max 25 cells per query; batch independent matrices in parallel
- Front-load the decision, hypothesis, and constraints into question instructions — the provider sees only state, instructions, and criteria; reasoning is trace-only

**Results:** Large resources auto-page; use each page’s scope, answer, confidence, and coverage together. Aggregate page-local verdicts before judging a file. Confidence calibrates the answer, not the question. Insufficient, uncertain, partial, or errored means narrow/read, not no.

**Safety:** Every verdict routes reading — it never proves identity, reachability, absence, or mutation safety. Do not use for deterministic recovery: invalid syntax, missing paths, auth/rate limits, unsupported capabilities, stale snapshots, or timeouts. When no bytes back the judgment (synthesis, self-review), treat the verdict as advisory, not proof.

In this repository replace `octocode` with `node packages/octocode/out/octocode.js`.
For setup → [ojql.md](references/ojql.md). For optional examples → [clasify-workflows.md](references/clasify-workflows.md).
