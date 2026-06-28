# Long Research

Read this when an Octocode research task needs a durable decision brief, claim-level traceability, or a frozen campaign plan. Keep the normal chat flow for small answers; this reference is for longer, contested, public, or multi-surface research.

## When To Use

Use this layer when at least one is true:

- The answer will influence a public roadmap, architecture decision, purchase, or deletion.
- Evidence spans 3+ surfaces, such as local code, GitHub, npm, web, PR history, and papers.
- Important claims conflict or need later audit.
- The user asks for a report, decision brief, research packet, or saved artifacts.

Do not use it for quick lookups, narrow code questions, or small prior-art maps. In those cases, keep a lightweight claim ledger in chat.

## Campaign Spec

Freeze the scope before deep work. A campaign spec can be a short markdown block in chat or a saved `research_campaign.json` only when the user wants artifacts.

Minimal fields:

```json
{
  "question": "What are the best options for structural TypeScript search?",
  "mode": "map",
  "surfaces": {
    "local": "skip: external landscape",
    "github": "active",
    "npm": "active",
    "web": "active for formal docs only",
    "history": "active for top repos if needed",
    "artifacts": "skip"
  },
  "budget": {
    "timeMinutes": 30,
    "maxReposDeepDived": 5,
    "maxSearchPasses": 3
  },
  "stopGates": [
    "top candidates have exact source evidence",
    "next search is unlikely to change ranking",
    "evidence conflict requires user weighting"
  ],
  "nonGoals": [
    "install packages",
    "make code changes"
  ]
}
```

Rules:

- Keep the campaign tiny. If a field is not useful, omit it.
- Update the spec only when scope changes, not after every search.
- Call out skipped surfaces so later readers know absence was a choice, not an oversight.
- Gate if the next step exceeds the budget or changes the public contract.

## Evidence Ledger

Use `evidence.jsonl` for proof items. Each row should be standalone and small.

```json
{"id":"ev1","type":"exact-file","source":"local","locator":"packages/a/src/foo.ts:42","quoteOrFact":"Function X calls Y","quality":"primary","retrievedAt":"2026-06-28"}
{"id":"ev2","type":"repo-readme","source":"github","locator":"owner/repo/README.md#section","quoteOrFact":"Project documents AST search support","quality":"primary","retrievedAt":"2026-06-28"}
```

Recommended fields:

- `id`: stable short id, such as `ev1`.
- `type`: `exact-file`, `lsp`, `ast`, `pr`, `commit`, `package`, `paper`, `official-doc`, `web`, `benchmark`, or `test`.
- `source`: `local`, `github`, `npm`, `web`, `paper`, or `artifact`.
- `locator`: path:line, URL, package version, PR/commit id, or artifact path.
- `quoteOrFact`: short fact, not a pasted article.
- `quality`: `primary`, `secondary`, `weak`, or `counter`.
- `retrievedAt`: date of retrieval for unstable sources.

Prefer exact source facts over broad summaries. Search snippets are leads; only promote them to evidence after exact read, source fetch, AST/LSP proof, package metadata, history, or tests.

## Claims Ledger

Use `claims.jsonl` for atomic claims and their support state.

```json
{"id":"cl1","claim":"Project A supports structural TypeScript search.","status":"supported","confidence":"confirmed","supportingEvidenceIds":["ev2","ev4"],"counterEvidenceIds":[],"nextCheck":null}
{"id":"cl2","claim":"Project B is actively maintained.","status":"partial","confidence":"likely","supportingEvidenceIds":["ev6"],"counterEvidenceIds":["ev7"],"nextCheck":"check releases and issue activity"}
```

Recommended fields:

- `id`: stable short id, such as `cl1`.
- `claim`: one sentence, one claim.
- `status`: `supported`, `partial`, `contradicted`, `unverified`, or `dropped`.
- `confidence`: `confirmed`, `likely`, or `uncertain`.
- `supportingEvidenceIds`: evidence rows that directly support the claim.
- `counterEvidenceIds`: evidence rows that weaken or contradict the claim.
- `nextCheck`: cheapest remaining proof step, or `null`.

Drop claims that are only plausible but unsupported. Keep contradicted claims when the contradiction matters to the decision.

## Workflow

1. Write or state the campaign spec.
2. Start a claim ledger before searching deeply: expected answer, alternatives, and disconfirming checks.
3. Run the normal `research-flow.md` surface plan.
4. Promote only proof-grade observations into `evidence.jsonl`.
5. Update claims after each decisive observation.
6. Run Advocate vs Critic over claims, not raw notes.
7. Produce the brief from supported and partial claims only.

## Vendor Adapters

Firecrawl, Tavily, Parallel, Perplexity, and similar tools are optional adapters. Use them only when they are available, authenticated, and useful for the active surface. They should enrich the web/paper surface; they do not replace Octocode proof for local, GitHub, npm, PR/history, AST/LSP, or artifact claims.

If a vendor adapter fails, continue with Octocode and runtime web search when possible. Mark the web/paper surface degraded instead of blocking the whole campaign.

## Output Skeleton

```text
TL;DR
Campaign scope
Claim ledger summary
Evidence by surface
What survived rebuttal
Verdict
Risks / unresolved claims
Recommended next step
Artifacts produced, if any
```

For chat-only answers, summarize the ledgers instead of dumping JSONL.
