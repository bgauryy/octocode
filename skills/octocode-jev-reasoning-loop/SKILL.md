---
name: octocode-jev-reasoning-loop
description: "Use when costly source reading can be offloaded before it enters agent context: pass Jev paths and bounded claims to choose the next check, or scout candidates to avoid irrelevant reads. Also review unresolved hypotheses or consequential plans. Use when avoided reading or deliberation repays the call; skip cheap exact checks."
---
# Octocode Jev reasoning loop

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, script, or scheme only when it changes the next action; otherwise use the entry point below.

Pass **where to read and what to judge**, before reading candidate bodies yourself. Octocode retrieves bounded, redacted source for Jev. The host chooses the next check and verifies the result. Confidence is not correctness.

Flow: `LOCATE PATHS → CHOOSE A USEFUL JUDGMENT → CHECK DECIDING EVIDENCE → ACT`.

## Source paths, then judgments

Discover paths with Octocode search, symbols or known locations. When semantic inspection would require substantial reading, use native `jevReasoning` with `route: "source_questions"`. Supply paths and independent affirmative claims; the runtime reads all sources into one shared state. No host-written evidence packet is needed.

Inspect only this route's schema: `octocode tools jevReasoning --scheme --scheme-view query --scheme-select route=source_questions --json --compact`.

```json
{"queries":[{"reasoning":"Choose the next cancellation regression to test","route":"source_questions","sources":[{"path":"/absolute/src/request.ts"},{"path":"/absolute/src/cache.ts"}],"questions":{"lateWrite":"A request cancelled after dispatch can still write its result to the shared cache."}}]}
```

Use observed paths. Optional line ranges narrow large files; optional `context` supplies the symptom or scope, not asserted proof. Claims share the sources but cannot depend on another answer. IDs carry no instructions. Invalid or oversized selections fail explicitly; narrow paths/ranges using the returned error.

Answers distinguish **supported, contradicted, insufficient and conflicting**, with probabilities and source fingerprints, without returning bodies. Use the judgment to choose a test or exact read. Inspect deciding evidence before source assertions or patches; tests can settle behavior directly. Uncertainty is not false. Never repeat unchanged votes or automatically chain routes.

## When another route saves more

- Many candidate files, only some worth reading → `jevScout`, `taxonomy: "relevance"`, `includeEvidence: true`. Supply the question as `claim` and paths in `source.local`. Inspect returned `read` and `gray_read` excerpts; widen missing or truncated spans. A skip never proves absence. The default taxonomy asks which files implement a capability. Details: `references/scout.md`.
- Evidence already inspected, but a consequential choice remains unresolved → the other `jevReasoning` routes; load `references/routing.md`. Reuse current evidence and counterevidence. Do not repackage already-read bodies to claim reading savings.
- Standalone typed aspects per file → `references/profile.md`; native source questions instead combine files.

A known anchor, exact lookup, cheap discriminating test or file you must read anyway usually warrants a direct check. Count preparation, schema reads, calls and follow-up reads when deciding whether Jev saved work. Judgments never grant authorization.

Use the same protocol for local code and pinned upstream checkouts; preserve repository/ref and source anchors. Already-read GitHub text offers no initial reading savings.

## Execution and artifacts

Keep inputs, responses, decisions and logs under `<output>/octocode-jev-reasoning-loop/`; scratch under `<output>/tmp/octocode-jev-reasoning-loop/`. Source edits retain their requested paths. Credentials → `references/configuration.md`.

Run `octocode tools jevReasoning --input .octocode/octocode-jev-reasoning-loop/request.json --json --compact`; `--input` takes a file, while inline JSON is positional. Full `--scheme` includes output contracts. Pin comparison models; measure host usage separately from provider usage.

Load details only as needed: evidence selection → `references/context.md`; packet debugging → `references/research.md`; primitives/limits → `references/protocol.md`; primary sources → `references/references.md`; measurement → `references/benchmark.md`. Standalone setup and packaging → `README.md`.

After runtime changes, run `npm test` and the `octocode-skills` reviewer.
