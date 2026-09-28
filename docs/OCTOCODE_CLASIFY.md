# Octocode Clasify

`clasify` is Octocode's only semantic tool. Jev is its classification provider, not a second tool.

Clasify has two modes:

- **Scout** executes an unread read-tool request, sanitizes the result, and returns typed judgments plus source receipts without returning source bodies.
- **Judge** classifies caller-supplied state in `context.value`; it performs no retrieval.

Clasify routes work. It does not prove source facts, global absence, symbol identity, reachability, or edit safety. Read the deciding source after a judgment.

## Admission

Use Clasify for an explicit classification request or a measured workflow that improves both answer quality and total host context. The admitted research route is `questionType:"locate"` over unread known files when the target is semantic and no useful literal is known. Direct search and bounded reads remain the default for literals, symbols, and already-known anchors.

The target budget is host-model context. Provider tokens are cheaper and tracked
separately; they may increase when private assessment prevents larger source
bodies from entering the host transcript. Do not optimize provider usage at the
cost of more host reads or weaker routing.

Skip Clasify when an exact lookup, existing evidence, direct reasoning, or one cheap read decides the next action. File count, file size, ambiguity, one search miss, or a shorter response alone are not admission reasons. Every call adds provider tokens, latency, and verification work.

Generic hydrated Scout remains experimental. Five held-out bug-fix tasks improved top-three routing recall but used 2.17× the complete-task host tokens and did not improve aggregate patch quality. Locate fixes the specific double-work defect: Jev now identifies an original-source range, so the host verifies that range without running a second search. Current evidence and limitations are preserved in [`.octocode/JEV.md`](../.octocode/JEV.md).

The external-doc locate run used four unread files × two independent semantic targets. It returned both deciding ranges correctly, then exact verification used 4,708 host-visible response bytes versus 5,915 for the search-first route, a 20.4% reduction. Provider work rose to 16,514 input and 3,172 output tokens, and final-run wall time rose from 461 ms to 3,597 ms. An earlier run took 1,486 ms, confirming network-sensitive latency while remaining slower than search. This proves transport leverage for this bounded workflow, not a broad agent-token or latency win; keep the route narrow and retain the five-bug A/B as the broader gate. Reproducible artifacts live under [`.octocode/octocode-eval-benchmark/clasify-external-corpus/`](../.octocode/octocode-eval-benchmark/clasify-external-corpus/).

Once a call is admitted, shared-state batching is proven useful. A real local
5-candidate × 3-question run used 6,415 provider input tokens in one matrix,
versus 16,715 for three one-question matrices (61.62% less), with no threshold
decision disagreements across 15 cells and three repeats. This optimizes an
admitted Clasify call; it does not establish an end-to-end research win. The
reproducible report is
[`.octocode/benchmarks/clasify-multi-question/REPORT.md`](../.octocode/benchmarks/clasify-multi-question/REPORT.md).
An external `tokio-rs/tokio` replay measured a 66.02% provider-input reduction
for the same shared-matrix shape and preserved commit-pinned verification reads.

## Architecture

```text
caller question
    │
    ├─ context.value ──────────────────────────────┐
    │                                              │
    └─ context.{tool,query}                        │
             │                                     │
             ▼                                     │
       validate + secure                           │
             │                                     │
             ▼                                     │
       bounded read capture                        │
             │                                     │
       search candidates?                          │
        ├─ search: snippets/metadata               │
        └─ fileChunks: bounded candidate reads     │
             │                                     │
             └─────────────────────────────────────┤
                                                   ▼
                                       independent Jev judgments
                                       locate: Choice(range) + Noul(exists)
                                                   │
                                                   ▼
                                 IDs + probabilities + source ranges + scopes
                                  + next.read / next.clasify
```

The core package owns the schema, descriptions, limits, and instructions. The native runtime validates the generated contract, executes delegated reads, sanitizes evidence, enforces capture/provider limits, and shapes continuations. CLI and MCP expose the same contract.

Delegated reads share a limit of four concurrent reads per call and sixteen per process. Search candidates hydrate concurrently inside those bounds. Provider judgments run independently through the provider's configured concurrency gate. Candidate order in the output remains deterministic.

## Availability

- Credential: `OCTOCODE_CLASSIFICATION_API`, else the vendor key `OCTOCODE_JEV_KEY`, else `.octocoderc` `classification.api`
- Optional HTTPS host override: `OCTOCODE_CLASSIFICATION_API_HOST`
- Provider/model family: Jev; resolved model and usage remain internal telemetry
- MCP registers `clasify` only when the credential is nonblank at process start.
- CLI remains callable without a credential and returns an actionable configuration error.

Credential resolution is process environment → workspace `.octocode/.env` → global Octocode `.env` (`~/.octocode/.env`, or `$OCTOCODE_HOME/.env`) → private `.octocoderc` `classification.api` (workspace `.octocode/.octocoderc`, then global). CLI and MCP load both files; no project-trust flag is needed for dotenv. Missing or blank file values fall back to the next source. An explicitly empty or whitespace process `OCTOCODE_CLASSIFICATION_API` disables Clasify even when file/vendor-key fallbacks exist. Restart MCP after changing configuration so clients refresh their catalog. Never put credentials in requests, logs, benchmark artifacts, or committed configuration.

```bash
npx octocode config --json
npx octocode scheme clasify --view query --compact
```

## Input

One semantic matrix contains:

| Field | Meaning |
|---|---|
| `id` | Stable query correlation ID |
| `reasoning` | Required, at most 500 characters. Sent in each page's evidence state. Say what the next read depends on |
| `goal` | Required, at most 500 characters. Sent with every question and in each page's evidence state. Say what a useful file must contain |
| `resources[]` | Independently captured state or unread read requests |
| `questions[]` | Caller-authored questions applied to every resource page |

Put all independent questions that use the same evidence in one matrix, and put
every candidate in `resources`. Each resource is captured once, and the runtime
batches all fitting questions for its page. Jev does not see the caller
transcript: required `goal` travels with every question and in each page's evidence state, and required `reasoning` travels in that same state. Use root `queries[]` for independent matrices whose resource cross-product
would be wrong. Use a later call only when an earlier answer changes the evidence
or available options. Resource and question IDs must be unique inside their
query.

A resource contains exactly one context form:

```json
{"id":"held","context":{"value":{"claim":"...","evidence":["..."]}}}
```

```json
{"id":"unread","context":{"tool":"localFetch","query":{
  "reasoning":"Capture the deciding implementation section.",
  "path":"/abs/repo/src/file.ts",
  "startLine":40,
  "endLine":100
}}}
```

The nested read query requires its own `reasoning`. Use the live schema for each read tool rather than copying old examples.

### Questions

Each question uses one primitive:

| Primitive | Meaning |
|---|---|
| `noul` | Probability of “yes” for one proposition |
| `choice` | One caller-defined label plus the complete probability map |
| `score` | Expected zero-based level on one ordered dimension |

Choice and Score confidence measures probability concentration, not correctness. Add an explicit `insufficient` label when substantive labels may not fit. There is no universal score or confidence threshold for discarding candidates.

Research presets are `locate`, `contribution`, `addsEvidence`, and `supportsClaim`. Do not combine a preset with custom `type`, `instructions`, or `criteria`.

`locate` accepts only `target` and applies to a contiguous original-source `localFetch` or `ghGetFileContent` page. The runtime tags small source passages, asks Jev a Choice question to rank them and a Noul question to estimate whether an answer exists, then projects the answer back to original line numbers. Where the engine outlines the language, passages are grouped by their innermost declaration (leading doc comment included) and a doc-comment hit shows the declaration line. The answer is `{exists,matches:[{startLine,endLine,probability}]}` with one match, or two when the page answers (`exists` ≥ 0.5) and the runner-up holds at least half the winner's probability. Each query's `best[questionId]` ranks windows across pages by `exists`, then `probability`. It is shown when the top `exists` is at least 0.5, or when the walk has no `next.clasify`. While a continuation remains and the top `exists` is lower, that ranking travels only as `carry`, so follow the continuation instead of reading the closest non-answer. The last call ranks the whole file. Identifier-like targets add a `hints` entry pointing to localSearch. A file resource may add `prefilter:[terms]` of rare literals: the runtime reads the file once and judges the three densest 600-line windows. Terms that occur throughout the file cover it, and a distinctive search is then cheaper. A finished ranking always has a winner; low `exists` means the returned range is merely the closest passage.

For `locate`, request unminified file reads (`minify` omitted or `"none"`), or use `localSearch`/`ghSearchCode` with `candidateEvidence:"fileChunks"`. Hydrated chunks can still have gaps; those pages remain unsupported. Plain search snippets, repository/tree listings, AST/LSP results, history, and package metadata support the other question types. A matrix combining `locate` with an incompatible tool resource is rejected before retrieval or provider calls; split it into separate matrices. Supplied values and captured pages still undergo source-line validation.

`answers.matches` provides each question’s source coordinates once. These are verification windows around ranked passages, not guaranteed complete declarations or answers. Batch nearby windows into at most five ranges per read call; expand or follow the source if the deciding statement is absent. Even a high score needs source verification. Results contain hints, never captured bodies. MCP returns a single structured payload with empty text content.

Each question must describe one source-local fact. Split lists, conjunctions, and
facts expected in distant sections into separate questions over the same capture.
Add several questions when every answer changes a route, threshold, read, test,
or edit. Cheap questions still consume cells, question tokens, and host-visible
output; they cannot repair an irrelevant candidate set.

## Research flows and intent boundaries

| Intent | Use Clasify when | Keep direct Octocode evidence when |
|---|---|---|
| Locate an answer inside unread known files | The target is semantic, several candidate files or questions share the same capture, and the result routes exact line reads | A literal, regex, symbol, AST shape, or line anchor expresses the target directly |
| Route search candidates | Scope and lexical repair already produced a bounded ambiguous shortlist, and the result selects an exact read | Paths, snippets, AST, or LSP already identify the deciding file |
| Map several properties over candidates | Up to 25 independent source-local questions all apply to the same unread pages; put them in one matrix | Each property needs different search terms, files, ranges, or later evidence |
| Triage history | Several unread PR, issue, commit, or diff candidates need the same relevance/support check before an exact history read | The number/ref is known or one diff answers the question |
| Check held evidence | A small supplied evidence set needs a typed supported/contradicted/conflicting/insufficient disposition that changes the next action | The source already states the answer or the agent merely wants a summary |
| Detect incremental evidence | `addsEvidence` can distinguish a new fact, exception, or contradiction from current `knownEvidence` | Exact duplicate identity or location is mechanically decidable |
| Choose a hypothesis/test | A closed Choice, including `insufficient`, selects a discriminating next test | The task needs open-ended causal reasoning or a new hypothesis |
| Rank one ordered dimension | A Score rubric is explicit and a threshold drives routing | “Correctness,” relevance, or severity has no calibrated rubric or action policy |
| Prove identity, reachability, absence, or edit safety | Never | Use LSP, AST/topology, exact reads, tests, and coverage appropriate to the claim |

The useful research placement is **retrieve → classify the bounded ambiguous
set → verify the deciding source**. Classification before retrieval has no state
to judge. Classification after the host has already read every body cannot save
host context. Adding questions after retrieval helps only when the same captured
state can answer them independently.

Measure the complete conversion:

```text
host leverage = direct-workflow host tokens
              - Clasify-workflow host tokens

Clasify-workflow host tokens include tool results, repair calls, selected reads,
uncertainty reads, and final answer generation. Provider input/output tokens and
latency are reported separately.
```

For a quick diagnostic before a full agent A/B, replace host tokens with
host-visible response bytes and divide the bytes avoided by provider input
tokens. This is a transport proxy, not proof of model-context savings.

## Scout over search candidates

One unread `localSearch` or GitHub `ghSearchCode` resource fans its returned file entries into independent pages.

- Omit `candidateEvidence`, or use `"search"`, to judge only returned paths, snippets, and metadata.
- Use `"fileChunks"` only for an explicit experiment where snippets cannot route the next read.
- File-chunk mode hydrates at most five candidates on the current search page.
- Each candidate contributes at most 12,000 sanitized characters.
- Candidate chunks are judged independently; one candidate is never visible to another.
- The output contains no source body.

```json
{
  "id":"candidate-screen",
  "reasoning":"Test whether bounded hydration improves the next-read decision.",
  "resources":[{
    "id":"hits",
    "context":{
      "tool":"localSearch",
      "candidateEvidence":"fileChunks",
      "query":{
        "reasoning":"Find implementations of handler registration.",
        "path":"/abs/repo",
        "searchText":"registerHandler",
        "include":["src/**"],
        "pageSize":5
      }
    }
  }],
  "questions":[{
    "id":"contribution",
    "question":{
      "questionType":"contribution",
      "target":"Where is handler registration implemented?"
    }
  }]
}
```

Each usable hydrated page includes `next.read`, an exact `localFetch` or `ghGetFileContent` request for verification. A malformed or failed candidate may not have one. `next.clasify` continues the underlying search; copy it unchanged.

### Large local repositories

Constrain the search before classification:

1. Use an absolute root and the narrowest useful `include`, `excludeDir`, language, and lexical anchor.
2. Keep `pageSize` at five or below for file-chunk mode.
3. Use direct `localSearch`, AST, or LSP evidence when syntax or symbol identity already decides the file.
4. Follow `next.clasify` only when later search pages remain relevant to the stated decision.
5. Execute `next.read` for the candidate that changes the action; a bounded chunk does not cover the rest of the file.

Local hydration reads a bounded region around the first stable match. If no stable anchor exists, the page records that only the opening chunk was assessed.

### Large GitHub repositories

GitHub code search is index-limited and cannot establish global absence. Narrow by owner/repository, filename, extension, language, and useful keywords before classification.

File-chunk mode performs bounded candidate fetches concurrently. Each exact-read continuation is pinned to the observed commit SHA when the provider returned one, so verification reads the assessed revision rather than a mutable branch. Continue search coverage with `next.clasify`; do not infer repository-wide absence from one page or one score.

## Judge supplied state

Judge is appropriate when the caller already holds a small, sufficient evidence set and needs an explicit classification whose result changes the next read, test, or edit.

```json
{
  "id":"claim-review",
  "reasoning":"Classify how the held evidence bears on the exact claim.",
  "resources":[{
    "id":"evidence",
    "context":{"value":{
      "claim":"Aborting a queued task guarantees it never starts.",
      "observations":[
        "Waiting tasks may be removed by an abort signal.",
        "Tasks already started continue after abort."
      ]
    }}
  }],
  "questions":[{
    "id":"support",
    "question":{
      "type":"choice",
      "instructions":"Classify support for the exact claim.",
      "criteria":{
        "supported":"Evidence covers the complete claim.",
        "contradicted":"Evidence conflicts with the claim.",
        "conflicting":"Evidence supports and conflicts with the claim.",
        "insufficient":"A deciding condition is missing."
      }
    }
  }]
}
```

Do not copy unread bodies into `context.value`; use an unread read request so the host does not first pay to consume that source.

## Limits

| Limit | Value |
|---|---:|
| Queries per call | 5 |
| Resources per query | 25 |
| Questions per query | 25 |
| Expanded cells per query | 25 |
| Total caller cells per call | 50 |
| Default `maxChars` per resource | 80,000 |
| Hydrated candidates per search page | 5 |
| Sanitized characters per hydrated candidate | 12,000 |

Search fan-out is included in the 25-cell limit. Twenty-five questions fit only with one resource, because resources × questions must stay at or under 25 cells. Calls that exceed the dynamic expanded-cell bound fail before provider work rather than silently dropping candidates.

## Output and verification

Results are ordered as `queries[] → resources[] → pages[] → answers[questionId]`. Each page is one result row: its source plus `answers`, the score for every question on that page (`noul`, choice, score, or locate `exists` and `matches`). Provider telemetry stays outside the agent response. Each page carries:

- source location and observed version when available
- assessed source scope or transformed view
- typed answers or a typed error
- limitations
- optional `next.read`

Each resource reports `coverage`. Here, `complete` means every captured page in this call was judged. It does not mean an entire file, search result set, or repository was read. Page limitations name unread content.

Rules:

- Execute `next.clasify` unchanged when more selected coverage is needed. Read `best` when its top `exists` is at least 0.5, or when that continuation is absent. A lower `exists` on an open walk is withheld from `best` and kept in `carry`.
- Execute a deciding `next.read` unchanged and cite the original source, not the score.
- Treat `partial`, `error`, `insufficient`, content-firewall rejection, and mid-band scores as unresolved.
- Preserve disjoint ranges; transformed-view positions are not source coordinates.
- Source versions are observations; verify mutable source before asserting.
- Do not average page scores or synthesize a global verdict unless the caller explicitly owns that policy.

## CLI

```bash
npx octocode scheme clasify --view query --compact
npx octocode clasify --input request.json
npx octocode clasify --input request.json --pretty
```

In this repository:

```bash
node packages/octocode/out/octocode.js clasify --input request.json
```

Exit codes: `0` judged, `6` continuation available, `5` every resource errored, `2` invalid input.

## Removed names

`semanticAssess`, `semanticRerank`, `jev`, `jevScout`, and `jevReasoning` are not public tools or aliases. Use `clasify` and `next.clasify`. Historical evaluations live under `.octocode`; they are evidence, not current call instructions.

The live schema is authoritative. Inspect it before hand-authoring a request.
