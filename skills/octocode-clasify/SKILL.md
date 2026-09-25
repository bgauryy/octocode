---
name: octocode-clasify
description: "Use when an unresolved semantic judgment changes the next action, or screening ambiguous unread evidence avoids broad reads. Scout unread candidates or judge held claim plus evidence. Skip when direct reasoning or a cheap exact check decides; not for missing facts, proof, global absence, or summaries."
---
# Semantic assessment

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Flow: `INSPECT → SHAPE → ASSESS → VERIFY`. Clasify exists to keep unread bodies out of your context: the provider reads them, you get verdicts plus line scopes, then you read only the deciding scope.

## Decide whether a call helps

Name the unresolved decision and how different answers change the next read, test, or action. Skip when current evidence, direct reasoning, or a cheap exact check already decides. Vague meaning can benefit from classification; missing facts need retrieval or user clarification. Size, candidate count, one search miss, and draft length alone do not trigger calls.

1. **Known anchor** → search → fetch deciding lines; stop when the evidence answers the question.
2. **Noisy or empty results** → repair scope, filters, and useful synonyms first. Reranking cannot recover an empty candidate set. If vocabulary remains uncertain, discover plausible paths independently (`astSearch` files locally, `ghSearch` tree remotely), then consider bounded Scout of unread sections. A miss alone never triggers a scan or proves absence; without plausible candidates, clarify the concept or broaden the evidence source.
3. **Ambiguous search snippets** → consider `semanticRerank` only when those snippets can decide reading order. It does not see unseen bodies.
4. **SCOUT unread evidence** → use unread read-tool resources when titles, paths, metadata, snippets, and direct reasoning cannot settle which candidate or region to read, and screening changes that choice. These visible clues can select a read but cannot prove the relevance of unseen text. Ask one concrete relevance question for the research direction: a partial fact, constraint, counterexample, document, test, or caller may matter even if it does not fully answer the question or implement the behavior. Do not read bodies merely to prepare the call.
5. **JUDGE held evidence** → when a held claim remains unresolved and its disposition changes the next action, put that claim and the smallest sufficient source context, constraints, counterevidence, and uncertainty in `context.value`. Separate assumptions from observations. A judgment selects a revision, discriminating test, or verification read; it cannot supply missing facts or settle factual disagreement by voting.
6. **Symbol identity** → use `lspSearch` at an observed anchor, then inspect wrappers if needed.
7. **Verify** factual claims on source evidence. Reuse judgments; do not automatically chain scout and judge or rejudge unchanged evidence.

Every call adds provider tokens and latency. Select the smallest complete meaning-preserving section: use a heading-bounded section, declaration body, or targeted line range when known. Do not clip an arbitrary `maxChars` prefix or submit a huge file merely to avoid choosing a section. Compare the full workflow cost, including question construction, retries, and verification reads; reduced host response bytes alone do not establish token savings or equal quality.

## Questions (the provider reads literally)
- **Noul** = P(yes) of one proposition about the content: "Does this content contain a fact, constraint, or counterexample relevant to X?" — never "Is X true?" (leaks priors on absent evidence). For implementation lookup, ask whether it implements X; exploratory research also values docs, tests, and callers. Scores order reads; uncertain or partially covered candidates remain open. No universal score excludes a candidate.
- **Choice** = one named class; the runtime adds `insufficient` when absent. No other label may also mean absence ("none", "other") — write "explicitly disables auth". Confidence is distribution concentration, not correctness. Set routing thresholds for the cost of a wrong decision and verify factual claims.
- **Score** = one ordered dimension, 2–10 self-contained ordered levels; the expected zero-based level can be fractional. Inspect the distribution; neither rounding nor the score proves correctness.
- One aspect per question; put the decision and constraints in `instructions` (the provider sees only state, instructions, and criteria; `reasoning` is trace-only) — but never the expected answer or an example of it (that primes the verdict: a supported claim scored 0.76 "overclaim" when the question argued for it). No counting or arithmetic.
- Several outcomes that exclude each other (supported / overclaimed / contradicted) = one Choice, not several Nouls — separate Nouls overlap (overclaim fired 0.69–0.90 on contradicted claims).
- Start with one useful question. Batch additional independent questions only when each changes an action; questions cannot see other answers. `resources × questions` ≤ 25 cells; independent matrices go in root `queries[]`.
- Each resource is judged separately. Combine related facts in one `context.value` when judging their relationship; separate resources cannot compare each other. Relevance does not establish novelty: compare a candidate with the minimal already-known evidence to assess added facts, exceptions, or contradictions. Exact-byte dedup is cheaper when location cannot change meaning; uncertain or conflicting candidates stay open.

## Shapes
For unread files, send identifiers in `context.tool` + `context.query` (an absolute local `path`, or GitHub `owner`/`repo`/repository-relative `path` and optional `branch`), not file content in `context.value`. Octocode runs the ordinary read internally, applies its path/security and output rules, sends the sanitized evidence to Jev, and returns only verdicts and scopes. A GitHub browser URL must be split into those canonical fields; `context.value` is for state already held by the agent. Delegated GitHub file reads populate the same credential-scoped content cache as `ghGetFileContent`, so a later exact read can reuse them. Local reads revalidate and reopen the file to see edits; the OS may cache bytes, but there is no persistent localFetch response cache.

```json
{"id":"retry-evidence","reasoning":"Choose which ambiguous unread sections to inspect","resources":[
  {"id":"local-policy","context":{"tool":"localFetch","query":{"reasoning":"candidate section","path":"/abs/transport.md","startLine":40,"endLine":85}}},
  {"id":"upstream-client","context":{"tool":"ghGetFileContent","query":{"reasoning":"candidate section","owner":"o","repo":"r","path":"src/client.ts","startLine":80,"endLine":140}}}],
 "questions":[{"id":"relevance","question":{"type":"noul","instructions":"Does this content contribute a concrete fact, constraint, or counterexample about retry safety? Partial evidence counts; a keyword mention alone does not."}}]}
```
Replace illustrative identities and ranges with observed candidates and complete section boundaries; retain a known GitHub ref when repeatability matters.
Rerank uses `{id, question}`, not a typed clasify question. For an implementation lookup, a string can distinguish an implementer from a caller: `{"id":"impl","question":"Does this file implement X rather than only calling or testing it?"}`. For a named symbol ask whether the file *defines* X. For exploratory research, ask whether the snippet contributes a concrete fact, constraint, or counterexample; callers and tests can be relevant.

## Results
- Pages carry `answers.<questionId>`, coverage and available `source`, `scope`, `focus`, or `view` receipts. Read only `focus[questionId]` for the question being investigated; it is an initial lead, not complete support. Otherwise use verified source `scope`, preserving each disjoint `scope.lineRanges[]`. `view` positions describe transformed content and must never become source line numbers. Use explicit source labels from an outline or an exact lookup instead. Map resource IDs back to their original local path or GitHub owner/repo/path/ref. `source.evidenceHash` identifies the sanitized assessed capture, not original file bytes; source changes invalidate a prior routing judgment. Aggregate pages before judging a file.
- The shapes above cover the common call. Use `scheme clasify --view query --compact` when a field is unclear or a call is rejected; the full `scheme clasify --compact` also includes examples and repeats the schema. Exit 6 means more coverage in `next.clasify`, not failure.
- `partial`/`error`/`insufficient`/mid-band = narrow or read, never "no" (clear negatives can still sit near 0.4 — read the deciding scope). Run `next.clasify` unchanged for remaining coverage.
- **Screen `lowSignal`:** no strong match was found under the runtime's diagnostic threshold. It does not exclude candidates or prove the answer is outside the set. Recheck the question and coverage, then widen or read directly as appropriate.
- **`classificationContentBlocked`:** the provider's content firewall refused that page (2 of 6 GitHub READMEs in one run). It is not a negative — read the page directly or judge a narrower line window.
- **Rerank** reorders `files[]` only; no file is removed. A poor `searchText` makes it useless (fix the search, not the question). The runtime adds implementer-versus-caller criteria only to explicit implementation or definition lookups; general relevance questions keep their scope. `lowSignal` means no strong snippet match was found; this is not an absence finding. Scores are ordering hints, not Noul thresholds. `semanticRerank.candidates[i]` = `files[i]` (`path`, `score`); `model`/`usage` report the cost. Read enough promising and competing leads to settle the decision; callers and tests that mention X can outscore its implementer, and code not in matched snippets cannot be seen. Consider an unread file screen only if the decision gate above holds. `localSearch` needs `resultView:"paginated"`; continue with `semanticRerank.next`.
- Supplied `value` may be large (the request cap is 4 MiB); a search page over `maxChars` fails with `classificationContextTooLarge` — lower `pageSize` or give candidates their own resources; `classificationStateTooLarge` = one page exceeded the provider window (lower `maxChars` or use a line window).

**Safety:** verdicts route reading — never proof of identity, reachability, absence, or mutation safety. Confirm any claim on fetched bytes; exact search tests absence. Without backing bytes (self-review) a verdict is advisory.

**Invoked from other tools:** apply the same decision gate to "clasify when:" hints and SCREEN steps in research, scraping, or browser workflows. An available classification step is optional when existing evidence already decides.

In this repository replace `octocode` with `node packages/octocode/out/octocode.js`.
For setup → [ojql.md](references/ojql.md). For optional examples → [clasify-workflows.md](references/clasify-workflows.md).
