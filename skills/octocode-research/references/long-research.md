# Long Research

**Rare path — skip by default.** Read only when Octocode research needs a durable decision brief, claim-level traceability, or a frozen campaign plan.
Keep the normal chat flow for small answers. Evidence grades and the router still come from `references/algorithm.md`.

## When To Use

Use this layer when at least one is true:

- The answer will influence a public roadmap, architecture decision, purchase, or deletion.
- Evidence spans 3+ surfaces, such as local code, GitHub, npm, web, PR history, and papers.
- Important claims conflict or need later audit.
- The user asks for a report, decision brief, research packet, or saved artifacts.

Do not use it for quick lookups, narrow code questions, or small prior-art maps. In those cases, keep a lightweight claim ledger in chat.

## Campaign Spec

Freeze scope before deep work. Use a short chat block, or save `research_campaign.json` only when the user wants artifacts.

| Field | Purpose |
|---|---|
| `question` | Exact decision or research question. |
| `mode` | `map`, `validate`, `investigate`, `plan`, `review`, `change`, or `loop`. |
| `surfaces` | Active/skipped sources with a reason for each skip. |
| `budget` | Time, max deep dives, and max search passes. |
| `stopGates` | Conditions that end or pause the campaign. |
| `nonGoals` | Work explicitly out of scope. |

```json
{
  "question": "What are the best options for structural TypeScript search?",
  "mode": "map",
  "surfaces": {"local":"skip: external landscape","github":"active","npm":"active","web":"formal docs only"},
  "budget": {"timeMinutes":30,"maxReposDeepDived":5,"maxSearchPasses":3},
  "stopGates": ["top candidates have exact source evidence","next search will not change ranking"],
  "nonGoals": ["install packages","make code changes"]
}
```

Rules:
- Omit fields that do not help the decision.
- Update the spec only when scope changes, not after every search.
- Call out skipped surfaces so absence is traceable.
- Gate if the next step exceeds the budget or changes the public contract.

## Evidence Ledger

Use `evidence.jsonl` for proof items. Each row should be standalone and small.

```json
{"id":"ev1","type":"exact-file","source":"local","locator":"packages/a/src/foo.ts:42","quoteOrFact":"Function X calls Y","quality":"primary","retrievedAt":"2026-06-28"}
{"id":"ev2","type":"repo-readme","source":"github","locator":"owner/repo/README.md#section","quoteOrFact":"Project documents AST search support","quality":"primary","retrievedAt":"2026-06-28"}
```

Field values: `type` ∈ exact-file|lsp|ast|pr|commit|package|paper|official-doc|web|benchmark|test; `source` ∈ local|github|npm|web|paper|artifact; `quality` ∈ primary|secondary|weak|counter; `locator` = path:line, URL, package version, PR/commit id, or artifact path; `quoteOrFact` = short fact, not a pasted article; `retrievedAt` for unstable sources.

Promote a search snippet to evidence only after exact read, source fetch, AST/LSP proof, package metadata, history, or tests.

## Claims Ledger

Use `claims.jsonl` for atomic claims and their support state.

```json
{"id":"cl1","claim":"Project A supports structural TypeScript search.","status":"supported","confidence":"confirmed","supportingEvidenceIds":["ev2","ev4"],"counterEvidenceIds":[],"nextCheck":null}
{"id":"cl2","claim":"Project B is actively maintained.","status":"partial","confidence":"likely","supportingEvidenceIds":["ev6"],"counterEvidenceIds":["ev7"],"nextCheck":"check releases and issue activity"}
```

Field values: one sentence per claim; `status` ∈ supported|partial|contradicted|unverified|dropped; `confidence` ∈ confirmed|likely|uncertain; `nextCheck` = cheapest remaining proof step, or `null`.

Drop claims that are only plausible but unsupported. Keep contradicted claims when the contradiction matters to the decision.

## Workflow

1. Write or state the campaign spec.
2. Start a claim ledger before searching deeply: expected answer, alternatives, and disconfirming checks.
3. Run the normal `research-flow.md` surface plan.
4. Promote only proof-grade observations into `evidence.jsonl`.
5. Update claims after each decisive observation.
6. Run Advocate vs Critic over claims, not raw notes.
7. Produce the brief from supported and partial claims only.
8. On a durable finding, store one distilled lesson with the host's memory tooling when available.
   Use `memory_record(...)`, `memory_reflect(...)`, or the `octocode-awareness` capture flow. If no reusable lesson survived rebuttal, skip memory capture.

## Vendor Adapters

Optional web-enrichment adapters, such as Tavily or Perplexity, help only when available and authenticated.
They enrich the web/paper surface; they do not replace Octocode proof for local, GitHub, npm, history, AST/LSP, or artifact claims.
If an adapter fails, continue with native web search and mark the surface degraded.

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
