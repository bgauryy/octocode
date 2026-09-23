# Octocode Clasify — LLM Classifier Reference

**`clasify` is Octocode's LLM classifier.** It turns unread candidates and supplied state into typed, body-free verdicts that route the agent's next read — without consuming model context on files it would otherwise have to open. The current classifier provider is **Jev** (TypeSafe System One). This document covers every way clasify is wired into the research stack and how to use it directly.

---

## Table of Contents

1. [Why and how we use clasify](#why-and-how-we-use-clasify)
2. [Provider: Jev](#provider-jev)
3. [Availability and credentials](#availability-and-credentials)
4. [Primitive types: Noul, Choice, Score](#primitive-types-noul-choice-score)
5. [Use 1 — Scout: screen many candidates before reading](#use-1--scout-screen-many-candidates-before-reading)
6. [Use 2 — Judge: score supplied context or drafts](#use-2--judge-score-supplied-context-or-drafts)
7. [Use 3 — Rerank: semantic reranking inside ghSearch and localSearch](#use-3--rerank-semantic-reranking-inside-ghsearch-and-localsearch)
8. [Use 4 — Skills: chrome-devtools, scraping, and RFC](#use-4--skills-chrome-devtools-scraping-and-rfc)
9. [Input shape](#input-shape)
10. [Hard limits enforced by contract](#hard-limits-enforced-by-contract)
11. [Output shape and pagination](#output-shape-and-pagination)
12. [Research workflow: INSPECT → SHAPE → ASSESS → VERIFY](#research-workflow-inspect--shape--assess--verify)
13. [What clasify cannot do](#what-clasify-cannot-do)
14. [CLI quick reference](#cli-quick-reference)
15. [Migration from semanticAssess / Jev alias](#migration-from-semanticassess--jev-alias)

---

## Why and how we use clasify

**Goal: the agent loads less context at equal answer quality.** Clasify moves reading off the agent: the runtime fetches and sanitizes each unread resource, the provider judges it, and the agent gets a verdict plus a line scope, then reads only the deciding scope. Efficiency and quality are the two acceptance criteria; everything below was chosen by measurement (2026-09-23, jev-1.13.0; frozen eval harnesses under `.octocode/octocode-eval-benchmark/clasify-final-2026-09-23/`).

### When it pays — and when it does not

The deciding question is *where the evidence lives*. Clasify pays when it sits in **unread bodies**; it costs when a literal anchor, a title, or a returned snippet already decides.

| Situation | Pattern | Measured |
|---|---|---|
| Deciding region inside large unread files and one exact search already missed | whole-file `localFetch`/`ghGetFileContent` resources × 1–3 questions, then narrow the winning ~600-line scope | −60% host bytes when three 5k-line React files competed; **+155% when forced** where a remembered anchor existed |
| Many candidate files, no anchor decides | `files` view → ≤25 whole-file resources × 1 Noul | −84% host bytes; right file 0.97 |
| Several READMEs compared on one capability | README resources × 1 Noul per capability | −92% host bytes (15k provider tokens) |
| Judge long evidence already held (diff, draft, many claims; >~80 lines) | `context.value` + one Choice `supported`/`overclaimed`/`contradicted` | 11/12 correct; separate Nouls overlap; short excerpts: judge yourself |
| Lexical page buries the implementer and its code shows in snippets | `semanticRerank`, 1 question | top-3 20/20, MRR 0.48 → 0.92 (held-out) |
| One literal anchor (typed or remembered) / title / snippet decides | **skip clasify** | forcing it cost ~3× host bytes and 2.5× time with no accuracy change; blind agents following the skill skipped correctly |

**Unfamiliar-code eval (3 low-star repos + an obscure local package, 8 anchor-free questions):** blind agents again never needed clasify — they mapped each behavior to a domain term (`tempo`, `BranchInst`, `CAPACITY`) and hit it in 1–2 calls, so routing correctly skipped (8/8 accuracy both arms). Forced clasify with `focus` windows landed on the right ~40 lines 4/4 and used −70% host bytes vs. plain tools (14.1 KB vs 47.1 KB) at 17–26k provider tokens per file. Conclusion: clasify is a strong **fallback after an exact search misses** on large files and for multi-file/README screening, not a default first step.

**Critical caveat:** in a comprehension eval designed to be anchor-free, agents still found literal anchors on every task (synonym grep locally, training memory on famous repos). Clasify's measured wins are therefore narrow — multi-large-file region finding and README comparison — and unproven on unconventional code.

Blind A/B (ordinary tools vs the skill's routing, 6 local + 6 GitHub fresh questions, 12+12 agents): accuracy 6/6 = 6/6 in both; GitHub host bytes 0.48× baseline (ACCEPT), local 1.30× (no clasify call was warranted; the gap was search-style noise). The measured lesson: on local code, lean search views (`matchOnly`, `include`) move bytes more than clasify; clasify is the lever when bodies are large and anchors are unknown.

### Locate cascade (scenario benchmark, 2026-09-23)

Seven "which file implements X?" scenarios, each run six ways, with ground truth fixed first: lexical scout → fetch (**A**), A plus reading every definition hit (**A+**), A plus clasify over the host's own summary (**B**), clasify screening the unread candidates (**C**), `semanticRerank` (**D**), and read-everything (**E**).

| Scenario | A | B | C | D | E |
|---|---|---|---|---|---|
| Clean anchor | ✓ 1.6K chars, 85 ms | ✓ | ✓ but 30K provider tokens | ✓ 0.9K | ✓ 49K |
| Top hit is a thin wrapper | ✗ | ✗ | ✓ | ✓ 1.2K | ✓ 54K |
| 18 candidates, no anchor | ✗ | ✗ | ✓ 0.92 | ✓ | ✓ 311K |
| Code uses other words | ✗ | ✓ filename guess, 0.51 | ✓ 0.89 | ✗ recall miss | ✓ 245K |
| Many callers + a re-export | ✗ | ✗ | tie 0.93/0.90 → read both | ✗ | ✓ 115K |
| Remote, several files | ✓ | ✓ | ✓ | — | ✓ 58K |
| Answer outside the candidates | ✗ | ✗ | all ≤0.22 → correct "none" | — | misleading |

The cheapest path that still decides wins: anchor → search → fetch; ambiguous page → `semanticRerank`; zero/off-target hits, no anchor, or large candidates → clasify screen; identity → `lspSearch` (one hop: it resolved the re-export case but landed on the wrapper in the wrapper case); long held evidence → clasify judge; verify on fetched bytes. Clasify over the host's own short summary is not a locating step — it guesses from file names.

Runtime support: a screen whose candidates all score ≤0.3 with complete coverage returns `lowSignal: [questionId]` on the query — widen the scope instead of reading the top file. A page the provider's content firewall refuses returns `classificationContentBlocked` (not a negative; read it directly).

### How we use the Jev API

| Choice | Why |
|---|---|
| **Batch every question over one state into one request** | Jev ingests state once; each extra question costs ~46 tokens, and batched answers equal single-question answers. Headroom: 72 KiB per state + question, 120 KiB per group. |
| **Send evidence only** (`{repo?, path, lines, content}`) | Page metadata (absolute base, timestamps, byte counters, pagination) was 26–30% extra provider tokens and ate `maxChars`. Now −21% tokens on the same matrix, same verdicts. |
| **Focus windows (provider line-search pattern)** | A one-resource, one-page, one-Noul file matrix sends its verdict and a speculative Choice over ~40-line windows plus `insufficient` in one request when batching fits; candidate matrices and multi-page files retain the positive-only follow-up so negative pages do not pay for unused window answers. Only a Noul ≥0.8 and a confident real window Choice yield `focus`. A same-file prototype used 10,385 input tokens instead of 20,175 while keeping the verdict and selected lines 481–520; broader accuracy remains to be measured. |
| **Coalesce adjacent file pages to ~24 KiB (~600 lines)** | Needle accuracy stays ≈0.95 up to ~30k tokens, but the verdict should localize: 100-line pages gave 20 fragments for 3 files (36 KB output); 48 KiB judged a 1,079-line README as one scope. 24 KiB gives ~600-line scopes for +5% tokens. |
| **Map `max_tokens_exceeded` → `classificationStateTooLarge`** | Jev's window is 32k tokens of state + longest question; the hint says lower `maxChars` or use a line window. Oversized *search* pages fail fast with `classificationContextTooLarge` instead of sending truncated JSON. |
| **Auto-add `insufficient` to Choice** | Without it an irrelevant state forced a wrong answer at confidence 1.0; with it `insufficient` wins 3/3. |
| **Resource-major, hoisted output** | `resources[].pages[].answers[questionId]`, `model`/`usage` once per query, no echoed legend/type, probabilities ≥0.01 rounded to 3 decimals: 36 KB → ~1.3 KB for a 3-file × 2-question matrix (51–80× smaller than the judged content). |
| **Only same-tool continuations** | Cross-tool drill-downs (`readPr`, `get*`) are suggestions, not remaining coverage; treating them as pages produced false `partial` and loops. |
| **Rerank implementer criteria + `lowSignal`** | Behavior questions get Noul criteria true = "does or defines it", false = "only calls, imports, tests, configures, documents, or mentions it" (skipped when the question itself targets tests/docs/callers). Held-out top-1 5/6 → 6/6 (the express `redirect` definition now beats 7 call sites); frozen 2-question MRR 0.917 → 0.952; 1-question unchanged except one all-≤0.27 page, which now reports `lowSignal`. |
| **Rerank reorders only** | `minScore` trimming was removed: true targets scored as low as 0.18, trimming never improved top-1, and a removed file is a silent recall loss. Candidates carry `path`, the sidecar reports `model`/`usage`, and the next-page call moves (not copies) out of `data.next`. |
| **Blank key = off** | `OCTOCODE_CLASSIFICATION_API=""` in the process env disables clasify and `semanticRerank` with no `.env`, `.octocoderc`, or `OCTOCODE_JEV_KEY` fallback. |

### Provider patterns we adopted (docs.typesafe.ai)

| Vendor pattern | Where it lives in octocode |
|---|---|
| Speculative fan-out — many questions, one state, one request | Questions over one page are batched (~46 tokens per extra question) |
| Line search — Choice over line/window IDs + an "exists" Noul | `focus` windows on high-scoring file pages |
| Re-ranking — one Noul per query × candidate with true/false criteria | `semanticRerank` with implementer criteria |
| Confidence-gated routing — thresholds scale with the cost of acting | Noul ≥0.8 read first / ≤0.2 skip; Choice/Score ≥0.9 accept, 0.5–0.9 lead |
| Choice relative vs Noul absolute (skill suggestion) — Choice picks *which*, Nouls decide *whether any* | Page Noul gates the focus Choice; `lowSignal` when no rerank candidate clears 0.4 |
| Filter state first; literal reading; no counting/dates | Evidence-only state; "Does this content show X?"; arithmetic stays in code |

We did not adopt composite scoring with weights (averaged rerank questions are equal-weight) or the date/extraction recipes (no use case yet).

### Question best practices (measured)

- **Noul:** "Does this content show/implement X?" — on absent evidence it scores 0.03–0.11; "Is X true?" drifts to 0.12–0.76 (model prior). Act ≥0.8, skip ≤0.2, read between; negatives can sit near 0.4.
- **Choice:** explicit labels; never an absence-shaped label (`none` won wrongly 2/3 on irrelevant state).
- **Score:** 3 levels for relevance (as stable as 4, one fewer boundary); round, never interpolate.
- **Front-load context:** the decision and hypothesis in `instructions` fixed 2/3 misroutes; `reasoning` never reaches the provider.
- **Rerank phrasing:** behavior → "Does this file implement X rather than only calling or testing it?"; named symbol → "Does this file contain the definition of X (not a call site like X(...))?" (definition .97 vs callers ≤.05).

### Cost and speed

~2.5k provider tokens per 200-line file; ~28k for three ~1k-line files; rerank ~3–4k tokens per 8-file page. Latency 1–4 s locally, 10–17 s for large GitHub files (sequential page fetches). Repeat variance ≤0.02.

## Provider: Jev

**Jev** is the internal name for the TypeSafe System One classifier family. It produces the three typed primitives — Noul, Choice, Score — that the clasify contract is built on.

- Public tool and CLI command: `clasify`
- Provider/model family: Jev (TypeSafe)
- Credential env var: `OCTOCODE_CLASSIFICATION_API`
- Optional host override: `OCTOCODE_CLASSIFICATION_API_HOST` (HTTPS only; loopback dev excepted)
- Results report the provider-resolved model once per query as `model` (e.g. `jev-1.13.0`; the internal request alias is `jev-latest`)

**There is no public `jev` or `semanticAssess` alias.** Calling either name fails at contract admission. The tool is `clasify`, the env var is `OCTOCODE_CLASSIFICATION_API`, and the provider happens to be Jev.

Provider primitive docs: [Noul](https://docs.typesafe.ai/primitives/noul) · [Choice](https://docs.typesafe.ai/primitives/choice) · [Score](https://docs.typesafe.ai/primitives/score) · [Advanced](https://docs.typesafe.ai/primitives/advanced)

---

## Availability and credentials

```bash
# Set the credential — never commit this
export OCTOCODE_CLASSIFICATION_API=<your-jev-api-key>

# Optional: override the API root (defaults to Jev's hosted endpoint)
export OCTOCODE_CLASSIFICATION_API_HOST=https://...

# Verify the tool appears in the live catalog
npx octocode scheme clasify --compact
```

**MCP:** `clasify` is registered only when `OCTOCODE_CLASSIFICATION_API` resolves to a nonblank value at process start. The `semanticRerank` addon on `ghSearch` and `localSearch` appears at the same time. Restart the process after adding or removing the credential. An empty value in the process env (`OCTOCODE_CLASSIFICATION_API=`) is an explicit opt-out: it disables `clasify` and `semanticRerank` even when `~/.octocode/.env`, `.octocoderc`, or `OCTOCODE_JEV_KEY` holds a key.

**CLI:** Always callable. Fails with an actionable error naming `OCTOCODE_CLASSIFICATION_API` when the key is absent.

**Do not put provider credentials in `.octocoderc` or commit them.**

---

## Primitive types: Noul, Choice, Score

Every clasify question uses exactly one of three primitives. One question = one primitive. One dimension per question — never hide a checklist inside one instruction.

| Primitive | Captures | `instructions` | `criteria` | Answer field |
|---|---|---|---|---|
| **Noul** | P(yes) for one binary proposition | Required non-empty string/object/array | Optional. If provided, supply both `true` and `false` (either may be `null`) | `noul` ∈ [0,1] |
| **Choice** | One named class from declared alternatives | Required | Required object, 2–255 distinct label keys; descriptions may be `null`. The runtime adds `insufficient` when absent | `choice` (winning label) + `probabilities` (entries ≥0.01) + `confidence` |
| **Score** | One ordered dimension | Required | Required array, 2–10 non-null level definitions (low → high) | `score` (zero-based expected level; index into your criteria) + `probabilities` (entries ≥0.01) + `confidence` |

**Confidence** on Choice and Score measures distribution concentration — how spread-out the probability mass is across alternatives. It is **not** the probability that the answer is correct. A high-confidence wrong answer is still wrong; verify.

---

## Use 1 — Scout: screen many candidates before reading

**The canonical clasify pattern.** Run a search, get 10–50 candidate files, screen them with clasify before reading any.

### When to use

- `ghSearch` returns many code hits and you need to pick which 2–3 files to actually open
- `localSearch` matches files across a large monorepo and most are unrelated
- `astSearch` / `lspSearch` returns a list of candidate reference sites
- `artifactSearch` returns several packages and you need to triage relevance
- `ghSearchHistory` returns many PRs or commits and you need to pick which to read

### Pattern

Each candidate becomes one `resource`. The same typed questions apply to all resources in the matrix. The runtime fetches and sanitizes each file without returning its body.

Pass a resource identifier, not its contents: `context.tool` selects an ordinary read, while `context.query` carries an absolute local path or GitHub `owner`/`repo`/repository-relative `path` (plus a `branch` when needed). Clone and tree materialization are direct acquisition actions, not clasify context. Convert a GitHub browser link to canonical fields; a URL by itself is not a `clasify` context. The runtime executes the read with the normal access checks and redaction, then sends the sanitized evidence to Jev. The agent receives verdicts and scopes, not the fetched body. `context.value` remains available for a draft or other state the agent already holds.

```json
{
  "id": "interceptor-scout",
  "reasoning": "20 ghSearch hits for 'interceptor ordering' — pick which files to read.",
  "resources": [
    {
      "id": "axios-core",
      "context": {
        "tool": "ghGetFileContent",
        "query": {
          "owner": "axios",
          "repo": "axios",
          "path": "lib/core/Axios.js",
          "fullContent": true,
          "reasoning": "Screen this candidate; do not return its body."
        }
      }
    },
    {
      "id": "axios-interceptor",
      "context": {
        "tool": "ghGetFileContent",
        "query": {
          "owner": "axios",
          "repo": "axios",
          "path": "lib/core/InterceptorManager.js",
          "fullContent": true,
          "reasoning": "Screen this candidate; do not return its body."
        }
      }
    }
  ],
  "questions": [
    {
      "id": "implements-ordering",
      "question": {
        "type": "choice",
        "instructions": "Does this file implement request interceptor ordering — the logic that determines which interceptor runs first?",
        "criteria": {
          "direct": "Directly implements or manages interceptor execution order.",
          "related": "Related to interceptors but not ordering specifically.",
          "unrelated": "Different concern entirely.",
          "insufficient": null
        }
      }
    },
    {
      "id": "read-priority",
      "question": {
        "type": "score",
        "instructions": "How strongly should this file be prioritized for an exact verification read?",
        "criteria": [
          "No signal — skip",
          "Weak lead — read only if direct candidates are exhausted",
          "Moderate — worth a targeted region read",
          "Strong — read this file next"
        ]
      }
    }
  ]
}
```

```bash
npx octocode clasify --input scout-request.json
```

**After screening:** fetch only the decisive lines from the top-ranked files. The clasify verdict tells you *where to look*, never *what the answer is*.

**Cache behavior:** delegated `ghGetFileContent` uses the ordinary GitHub provider and credential-scoped content cache. A subsequent exact `ghGetFileContent` call can reuse that entry, including across CLI processes when the persistent cache is enabled. A live scout of `octocat/Hello-World` `README` followed by a separate CLI read returned `cache: 1`. Local `localFetch` rechecks path policy and reopens the current file on each call; filesystem caching can avoid disk I/O, while the agent's later exact read still sees edits. There is no persistent local file response cache.

### Local file scouting

Use `localFetch` instead of `ghGetFileContent` for local candidates. For a whole file, omit `startLine`/`endLine`. For exact regions, add them:

```json
{
  "id": "region-scout",
  "context": {
    "tool": "localFetch",
    "query": {
      "path": "/absolute/path/to/file.ts",
      "startLine": 80,
      "endLine": 160,
      "reasoning": "Screen region 80-160 without returning its body."
    }
  }
}
```

Multiple regions from the same file use distinct resource IDs — the path may repeat, the ID must not:

```json
[
  { "id": "region-a", "context": { "tool": "localFetch", "query": { "path": "/f.ts", "startLine": 1,  "endLine": 40,  "reasoning": "..." } } },
  { "id": "region-b", "context": { "tool": "localFetch", "query": { "path": "/f.ts", "startLine": 81, "endLine": 120, "reasoning": "..." } } }
]
```

---

## Use 2 — Judge: score supplied context or drafts

**clasify over already-observed state.** No file read occurs. The agent passes the content it already holds as `context.value` and asks typed questions about it.

### When to use

- Agent holds a draft answer and wants to self-review for gaps or overclaims before delivering
- A diff summary, API change description, or migration plan is in context and needs risk triage
- Two sub-agent positions conflict and need a bounded settling vote
- Coverage or test summary is held in state and needs a targeted judgment

### Pattern

```json
{
  "id": "self-review",
  "reasoning": "Draft answer is in context — check for overclaims before delivering.",
  "resources": [
    {
      "id": "draft",
      "context": {
        "value": {
          "answer": "The interceptor list is reversed before execution.",
          "evidence": "Observed in axios/lib/core/Axios.js line 52.",
          "assumptions": ["Only applies to request interceptors, not response."],
          "uncertainties": ["Did not verify the response interceptor path."]
        }
      }
    }
  ],
  "questions": [
    {
      "id": "overclaim",
      "question": {
        "type": "noul",
        "instructions": "Does the draft make a claim that goes beyond what the cited evidence actually shows? Score high when an assertion covers more than the stated observation, or when an assumption is presented as fact."
      }
    },
    {
      "id": "gap",
      "question": {
        "type": "choice",
        "instructions": "What is the most important missing piece in this draft?",
        "criteria": {
          "missing-evidence": "A key assertion has no cited source.",
          "missing-scope": "The answer omits an important scenario or edge case.",
          "missing-none": "The draft is complete within its stated scope.",
          "insufficient": null
        }
      }
    }
  ]
}
```

### Independent matrices with `queries[]`

Use root `queries[]` only when the matrices are truly independent — different resources and questions whose cross-product would be wrong to mix:

```json
{
  "queries": [
    {
      "id": "api-risk",
      "reasoning": "Decide if the API change needs compatibility review.",
      "resources": [
        { "id": "diff-summary", "context": { "value": { "removedFields": ["legacyMode"] } } }
      ],
      "questions": [
        { "id": "breaking", "question": { "type": "noul", "instructions": "Does this state indicate a breaking API change?" } }
      ]
    },
    {
      "id": "test-risk",
      "reasoning": "Decide if focused failure-path tests are needed.",
      "resources": [
        { "id": "coverage", "context": { "value": { "failurePathsCovered": false } } }
      ],
      "questions": [
        { "id": "needs-tests", "question": { "type": "noul", "instructions": "Are focused failure-path tests needed given this coverage state?" } }
      ]
    }
  ]
}
```

**Do not use `queries[]` as a way to share a large resource across several questions** — that's what `resources[] × questions[]` is for. One matrix, one resource captured once, all questions applied.

---

## Use 3 — Rerank: semantic reranking inside ghSearch and localSearch

**Integrated semantic reranking** runs the Jev classifier on search results *in-band*, inside a single `ghSearch` or `localSearch` call. No separate `clasify` call is needed. The classifier scores candidate paths and their returned snippets; files are reordered by score before the agent decides what to read.

### How it works

- Add a `semanticRerank` object with 1–5 Noul questions to a `ghSearch` (`operation:"code"`, `match:"file"`, non-concise output) or `localSearch` (`resultView:"paginated"`) call
- The runtime evaluates each candidate against all questions server-side — no full-file read happens
- Scores are averaged equally across questions and used to sort the result list
- The original `files[]` entries are unchanged and none is removed; only their order changes
- The compact `semanticRerank` sidecar has one `candidates[i]` row per returned `files[i]`: `sourceRank` (lexical position), `path`, `score` (mean), per-question `scores` when there are several questions, and per-question `errors` when a score is missing. It also carries `totalCandidates`/`evaluatedCandidates`, the provider `model` and `usage`, and `next` for reranked paging. A failed search row gets no sidecar

```json
"semanticRerank": {
  "status": "success", "totalCandidates": 5, "evaluatedCandidates": 5,
  "candidates": [{ "sourceRank": 3, "path": "lib/core/Axios.js", "score": 0.29 }],
  "model": "jev-1.13.0", "usage": { "input_tokens": 4410, "output_tokens": 80 },
  "next": { "nextPage": { "tool": "ghSearch", "query": { "...": "...", "semanticRerank": { "...": "..." } } } }
}
```

### ghSearch with semantic reranking

```json
{
  "operation": "code",
  "keywords": ["interceptors", "request"],
  "owner": "axios",
  "repo": "axios",
  "match": "file",
  "semanticRerank": {
    "questions": [
      {
        "id": "direct",
        "question": "Does this file directly implement request interceptor execution ordering?"
      },
      {
        "id": "runtime",
        "question": "Does this file contain production runtime logic, not tests or documentation?"
      }
    ]
  }
}
```

### localSearch with semantic reranking

```json
{
  "searchText": "semanticAssess|clasify",
  "path": "/Users/me/code/octocode",
  "resultView": "paginated",
  "semanticRerank": {
    "questions": [
      {
        "id": "caller",
        "question": "Does this file call or invoke the clasify tool — not just reference its name in a comment or test?"
      }
    ]
  }
}
```

### Reranking rules and caveats

| Rule | Reason |
|---|---|
| **Prefer rank-only** | Scores are rubric-specific and not calibrated across searches: live, the correct `lib/core/Axios.js` scored 0.29–0.64 across runs. Read in reranked order and stop at the deciding file. |
| **Reorder only** | Reranking never removes a file (the former `minScore` trim was dropped: it removed true targets that scored as low as 0.18 and did not improve top-1). |
| **At most 8 files, 25 cells per query** | `pageSize` defaults to the maximum (8, or ⌊25 ÷ questions⌋) and a larger `pageSize` is rejected. |
| **One question = one aspect** | Questions are averaged with equal weight. Separate questions for alternative valid roles (e.g. "implements" vs "tests") penalize files that correctly satisfy only one role. |
| **Reranking routes reading, not relevance** | Fetch and verify the deciding source bytes. A high score does not prove the file is relevant. |
| **Continue with `semanticRerank.next`** | Exit code 6 means more pages remain. The reranked next-page call moves out of `data.next` into `semanticRerank.next` (no duplicate); later match pages of the same files stay in `data.next` and are not re-scored. |
| **Provider failure returns original order** | Unscored candidates keep source order; a total failure reports one sidecar `error` and never returns an empty result. |
| **Payload cost** | The sidecar is small (≈30 bytes per candidate); most added bytes are the duplicated `semanticRerank.next` continuation. Reranking saves downstream reads, not the initial response. |

---

## Use 4 — Skills: chrome-devtools, scraping, and RFC

clasify is wired as a **mandatory context gate** inside three skills. In all three cases the same principle applies: bodies stay on disk, verdicts arrive in chat, and the agent reads only what the verdict routes to.

---

### octocode-scraping — SCREEN step

**Flow:** `FRAME → POLICY → ROUTE → FETCH → CORPUS → SCREEN → CITE → RECOVER`

After every fetch, the **SCREEN step is mandatory** before reading any corpus page. This is the primary protection against context bloat.

```
Fetch 15 pages from docs site
         ↓
SCREEN: clasify each saved corpus part as unread localFetch resources
         ↓
route: read (2 files) / consider (4 files) / skip (9 files)
         ↓
Read only the 2 "read"-routed files + run corpus-find on the 4 "consider" files
```

**Standard SCREEN question set:**

| Question ID | Type | Instructions |
|---|---|---|
| `content-type` | Choice | `pricing-table` / `api-reference` / `docs-guide` / `marketing` / `listing` / `other` |
| `has-target-data` | Noul | Write one specific goal question, e.g. "Does this page contain Stripe payment fee percentages?" |
| `route` | Choice | `read` (extract now) / `consider` (run corpus-find first) / `skip` (follow links instead) / `cdp-needed` (JS-rendered — escalate) |

**Rules for the scraping SCREEN:**
- One `resources[] × questions[]` matrix — up to 25 cells, `maxChars: 20000` per resource
- **Skip 0-byte files before building the matrix** — empty files crash the output validator
- Accept a `route` Choice only when `confidence >= 0.5`; treat lower as `consider`
- On `consider`: run `corpus-find.mjs` lexical search first, then clasify only matching spans
- A `route` verdict is not evidence — read the deciding spans from kept files before citing anything

**Link routing (avoid crawling all links blindly):**

```json
{
  "id": "link-routing",
  "reasoning": "Decide which sections to crawl next from the extracted links.jsonl.",
  "resources": [
    { "id": "links-sample", "context": { "value": ["/docs/pricing", "/docs/api", "/blog/2024", "/about"] } }
  ],
  "questions": [
    {
      "id": "link-action",
      "question": {
        "type": "choice",
        "instructions": "Is this link worth crawling for the stated research goal (API pricing data)?",
        "criteria": {
          "crawl-section": "Directly relevant section — crawl next.",
          "spot-check": "Possibly relevant — one-page check only.",
          "stop": "Not relevant — skip."
        }
      }
    }
  ]
}
```

**HAR / network check (after CDP ingestion):**

```json
{
  "questions": [
    {
      "id": "has-text-bodies",
      "question": {
        "type": "noul",
        "instructions": "Do any entries contain HTML or JSON response bodies worth extracting — not just images, webfonts, or analytics beacons? Score 0.2 or lower if all entries are binary/tracking."
      }
    },
    {
      "id": "har-action",
      "question": {
        "type": "choice",
        "instructions": "What should happen next with this HAR?",
        "criteria": {
          "extract-bodies": "Text/JSON bodies present — run har-pager.",
          "navigate-more": "Only assets captured — navigate more pages.",
          "skip": "Nothing to extract."
        }
      }
    }
  ]
}
```

---

### octocode-chrome-devtools — SCREEN step

**Flow:** `OPEN/ATTACH → STEALTH → PICK ONE INTENT → run(cdp) → REUSE PORT/TAB → SCREEN → QUERY DISK → CLEANUP`

Same context gate as scraping, but over CDP-captured artifacts. Before reading any `cdp/body-*.txt`, `dom-check.json`, `graph-actionability.json`, or HAR-derived file, always run SCREEN.

**Standard DOM question set** (on `page-snapshot.json` which contains `{url, refs:{e1:{role,name},...}}`):

| Question ID | Type | Instructions |
|---|---|---|
| `has-product-nav` | Noul | "Does the DOM contain a product/section navigation menu with 5+ named links?" |
| `has-cta` | Noul | "Does the DOM contain primary CTA elements — sign-up, start-free-trial, get-started, checkout — that are operable?" |
| `has-pricing-elements` | Noul | "Does the DOM contain pricing table elements, plan names, or fee rows?" |
| `dom-intent` | Choice | `extract-nav-links` / `click-cta` / `extract-pricing` / `inspect-only` |

**Standard HAR question set:**

| Question ID | Type | Instructions |
|---|---|---|
| `has-text-bodies` | Noul | "Do any captured entries contain HTML, JSON, or text bodies worth extracting?" |
| `has-api-calls` | Noul | "Does the HAR contain XHR/fetch calls to an API endpoint (not analytics) that return structured JSON?" |
| `har-action` | Choice | `extract-bodies` / `navigate-more` / `replay-api` / `skip` |

**Link routing from `graph-actionability.json`:** Pass navigation nodes as `context.value` resources (not file reads) to avoid opening every link:

```json
{
  "id": "nav-routing",
  "reasoning": "Decide which navigation nodes to follow from graph-actionability.json.",
  "resources": [
    { "id": "nav-nodes", "context": { "value": [{"href":"/pricing","label":"Pricing"},{"href":"/docs","label":"Docs"},{"href":"/blog","label":"Blog"}] } }
  ],
  "questions": [
    { "id": "link-relevance", "question": { "type": "noul", "instructions": "Is this navigation node relevant to finding API pricing data?" } },
    { "id": "link-action", "question": { "type": "choice", "instructions": "What to do with this link?", "criteria": { "follow": "Navigate to it.", "spot-check": "One-page check only.", "skip": "Not relevant." } } }
  ]
}
```

**CDP escalation path (scraping → chrome-devtools):**
When `corpus-run --regex` returns zero matches but `has-target-data > 0.6`, the content is JS-rendered. Load `octocode-chrome-devtools`, run `open-browser + page-snapshot + dom-operations-check`, then bridge results back with `scripts/har-ingest.mjs --session-dir <existing-session>`. Do not start a new session.

---

### octocode-rfc-generator — Jev debate judge

In the RFC skill, clasify is used as a **bounded risk-prioritization judge** after two sub-agents independently argue and rebut. It is the tie-breaker of last resort — not the primary decision mechanism.

**Gate: call clasify only when all three conditions hold:**
1. The two agents' final positions still differ after debate
2. Inspected evidence and a direct check cannot settle the disagreement
3. Support vs. rejection changes the next action in the RFC

**The Jev review flow:**
1. Two workers independently assess the contested RFC question and produce written arguments
2. Each reviews the other's argument and produces a rebuttal
3. If positions converge → no clasify call needed; resolve from evidence
4. If positions still differ → submit a `SemanticQuery` with the frozen disagreement as `context.value`
5. The `clasify` verdict is a bounded risk-prioritization aid, not a proof; a judge vote never closes a blocker by itself

```json
{
  "id": "rfc-risk-triage",
  "reasoning": "Worker A and Worker B reached opposite conclusions on API compatibility risk. Direct evidence checks did not resolve it. Judge the disagreement.",
  "resources": [
    {
      "id": "debate-packet",
      "context": {
        "value": {
          "question": "Does removing the `legacyMode` field constitute a breaking change for current consumers?",
          "workerA": { "position": "breaking", "argument": "...", "rebuttal": "..." },
          "workerB": { "position": "non-breaking", "argument": "...", "rebuttal": "..." },
          "evidence": { "removedField": "legacyMode", "callers": 3, "callerDetails": "..." },
          "unresolved": true
        }
      }
    }
  ],
  "questions": [
    {
      "id": "compatibility-risk",
      "question": {
        "type": "choice",
        "instructions": "Based on the debate packet and evidence, which position has stronger support?",
        "criteria": {
          "supports-breaking": "Evidence supports Worker A — breaking change, needs migration guide.",
          "supports-non-breaking": "Evidence supports Worker B — safe to ship without migration.",
          "insufficient": null
        }
      }
    },
    {
      "id": "confidence-level",
      "question": {
        "type": "score",
        "instructions": "How confident should the RFC author be in proceeding without additional research?",
        "criteria": [
          "Very uncertain — block the RFC, gather more evidence",
          "Uncertain — add a risk note and a verification trigger",
          "Moderate — proceed with documented assumption",
          "Confident — proceed"
        ]
      }
    }
  ]
}
```

Run the preflight validator before submitting:
```bash
node scripts/validate-debate.mjs request.json worker-packet.json
```

---

## Input shape

Pass either one complete `SemanticQuery` directly or `{ "queries": [...] }` for independent matrices.

### SemanticQuery fields

| Field | Required | Description |
|---|---|---|
| `id` | ✓ | Stable correlation ID for this query |
| `reasoning` | ✓ | Non-blank trace metadata: why this judgment changes the next action |
| `resources` | ✓ | 1–25 resource objects, each with unique `id` + `context` |
| `questions` | ✓ | 1–5 typed question objects, each with unique `id` + `question` |

### Resource `context` — exactly one of:

```json
{ "value": <non-null, non-empty string|object|array> }
```
Already-observed state. Use for drafts, diff summaries, search hit metadata, or any content the agent already holds.

```json
{ "tool": "localFetch",          "query": { "path": "...", "reasoning": "..." } }
{ "tool": "ghGetFileContent",    "query": { "owner": "...", "repo": "...", "path": "...", "reasoning": "..." } }
```
Delegated read — the runtime fetches and sanitizes the file without returning its body. The nested query requires its own `reasoning`.

### Complete example

```json
{
  "id": "candidate-screen",
  "reasoning": "Three unread files compete for the next read; pick the most decisive.",
  "resources": [
    {
      "id": "file-a",
      "context": { "tool": "localFetch", "query": { "path": "/absolute/a.ts", "reasoning": "Screen without reading." } }
    },
    {
      "id": "file-b",
      "context": { "tool": "localFetch", "query": { "path": "/absolute/b.ts", "reasoning": "Screen without reading." } }
    }
  ],
  "questions": [
    {
      "id": "relevant",
      "question": {
        "type": "choice",
        "instructions": "Does this file directly implement the feature under investigation?",
        "criteria": { "direct": "Implements it.", "related": "Touches it indirectly.", "unrelated": "Different concern.", "insufficient": null }
      }
    },
    {
      "id": "priority",
      "question": {
        "type": "score",
        "instructions": "How strongly should this file be prioritized for the next exact read?",
        "criteria": ["Skip", "Low priority", "Medium priority", "Read next"]
      }
    }
  ]
}
```

Save to a file and run:
```bash
npx octocode clasify --input request.json
```

---

## Hard limits enforced by contract

These are enforced at schema admission — calls violating them are rejected before reaching the provider.

| Limit | Value |
|---|---|
| Max queries per call (`queries[]`) | 5 |
| Max resources per query | 25 |
| Max questions per query | 5 |
| Max cells per query (`resources × questions`) | 25 |
| Max total cells across all queries per call | 50 |
| Default `maxChars` per resource | 80,000 chars |
| Resource IDs must be unique within a query | ✓ |
| Question IDs must be unique within a query | ✓ |
| `reasoning` must be non-blank | ✓ |
| `context.value` must be non-null and non-empty | ✓ |
| `startLine` must be ≤ `endLine` | ✓ |
| Nested file queries must include `reasoning` | ✓ |
| Noul `criteria`: both `true` and `false` if present | ✓ |
| Choice `criteria`: 2–255 distinct label keys | ✓ |
| Score `criteria`: 2–10 non-null, non-empty level definitions | ✓ |

Inspect the live schema for the current authoritative limits:
```bash
npx octocode scheme clasify --compact
```

---

## Output shape and pagination

Results are resource-major: `queries[] → resources[] → pages[] → answers[questionId]`. The resolved `model` and summed provider `usage` appear once per query. File and history reads (`localFetch`, `ghGetFileContent`, `ghGetHistoryItem`) are paged automatically and adjacent pages are coalesced into token-safe provider judgments; a search or discovery resource captures only the requested page and returns the rest in `next.clasify`. Each page carries its `scope` (a contiguous line/byte span, or `lineRanges[]` for disjoint match windows) and one answer per question. Disjoint windows remain separate from adjacent-page coalescing and do not produce a misleading single `focus` line. Answers carry no `type`: the key names it (`noul`, `choice`, `score`, or `error`).

```json
{
  "queries": [
    {
      "queryId": "candidate-screen",
      "model": "jev-1.13.0",
      "usage": { "input_tokens": 3549, "output_tokens": 52 },
      "resources": [
        {
          "resourceId": "file-a",
          "coverage": "complete",
          "pages": [
            {
              "scope": { "startLine": 1, "endLine": 352, "totalLines": 352 },
              "answers": {
                "retry": { "noul": 0.97 },
                "role": { "choice": "transport", "confidence": 1, "probabilities": { "transport": 1 } }
              }
            }
          ]
        }
      ]
    }
  ]
}
```

A page that failed as a whole has `error` instead of `answers`; a single failed question on a page is `answers.<id>.error`.

### Pagination rules

- **Never drop pages.** Aggregate page-local verdicts before judging a file.
- **If `next.clasify` is present, execute it unchanged.** A partial result cannot establish global absence.
- **`coverage: "partial"` or `coverage: "error"` is not a negative verdict.** It means evidence remains or the provider failed. Narrow the resource or read that region directly.
- **`classificationStateTooLarge`** means one page exceeded the provider context window: lower `maxChars` or target a `startLine`/`endLine` window.
- The runtime does not average, vote, or reduce page answers — that is the agent's job.

### How far to trust a verdict

Measured on 40 labeled items from this repository (jev-1.13.0, 2026-09-23):

| Signal | Observed | Agent rule |
|---|---|---|
| Noul ≥0.8 / ≤0.2 | 15/15 correct | Act ≥0.8, skip ≤0.2, read 0.2–0.8 |
| Choice/Score confidence ≥0.9 | 15/16 correct | Accept; the one miss was a `none` label that also described absence |
| Confidence 0.5–0.9 | correct but weaker | Treat as a lead; read the deciding scope |
| Choice without `insufficient` | forced wrong answer at confidence 1.0 | Runtime now adds `insufficient` |
| "Is X true?" on missing evidence | 0.76 (model prior leaked) | Ask "Does this content show X?" (0.05–0.25) |
| Rerank (path + snippets), early prototype | target top-3 6/6, top-1 3/6 before implementer criteria; the later held-out check reported top-1 6/6 | Read the top 3; scores within 0.1 are ties |
| State size | needle found at 0.95 up to ~30k tokens | Over 32k tokens the provider rejects the page |

These are routing signals, not proof: confirm any claim on fetched bytes.

---

## Research workflow: INSPECT → SHAPE → ASSESS → VERIFY

```
INSPECT    What candidates exist? (search, AST, LSP, history)
           ↓
SHAPE      Frame one bounded decision. Pick Noul/Choice/Score.
           Keep resources × questions ≤ 25 cells.
           ↓
ASSESS     Run clasify. Collect all pages. Follow next.clasify.
           ↓
VERIFY     Fetch the decisive lines from the top-ranked resource.
           Cite the fetched bytes — not the semantic verdict.
```

### Decide at SHAPE: clasify or not?

Use clasify when:
- Many unread candidates compete for a read and reading all would consume model context
- One large unread resource needs region-selection before reading
- Supplied state (draft, diff, debate) needs bounded typed judgment

**Skip clasify when:**
- The anchor is already known → use `localFetch` or `ghGetFileContent` directly
- The question is exact (does this line contain X?) → use `localSearch` or `corpus-run --regex`
- A deterministic check settles it (file exists, version matches, syntax valid) → use shell/search
- Every candidate must be read anyway (e.g. only 2 files)
- The answer is already known from a previous step

### Batching independent probes

Run independent clasify queries in parallel alongside independent research probes. Keep dependent judgments sequential — do not run a second clasify query before acting on the first when the second depends on the first's verdict.

```
Parallel batch:
  [clasify: scout 20 ghSearch hits]  +  [localSearch: find symbol anchors]
              ↓                                    ↓
          verdict                            line anchors
              ↓                                    ↓
  [localFetch: read top-3 files]  +  [lspSearch: resolve identity]
```

---

## What clasify cannot do

| Cannot do | Use instead |
|---|---|
| Prove exact text presence | `localSearch` / `corpus-run --regex` |
| Resolve symbol identity | `lspSearch` |
| Prove global absence | Read all candidates + exact search |
| Authorize a structural rewrite | `astRewrite` (requires snapshot + evidence) |
| Recover from deterministic errors | Handle invalid syntax, missing paths, auth failures with direct checks |
| Replace reading | Always verify decisive bytes after a verdict |
| Judge stale snapshots as current | Re-fetch when currency matters |
| Prove safety of deletion | `astTopology` + `lspSearch` callers |
| Produce a free-form summary or explanation | Use the model directly |

**Never assert a finding or write an audit conclusion from a score alone.** A `choice: "direct"` verdict routes you to read the file; it does not prove the file implements the feature. Fetch the deciding bytes, copy literals from their defining lines, and cite those — not the semantic result.

---

## CLI quick reference

```bash
# Inspect the live contract before hand-authoring calls
npx octocode scheme clasify --compact
npx octocode scheme clasify --view query --compact   # query schema only

# Run a saved request
npx octocode clasify --input request.json

# Output is single-line JSON by default; add --pretty to indent for humans
npx octocode clasify --input request.json --pretty

# Exit codes: 0 judged · 6 more coverage in next.clasify · 5 every resource errored · 2 invalid input

# Check that the credential is wired
npx octocode config --json | grep CLASSIFICATION
```

In this monorepo:
```bash
node packages/octocode/out/octocode.js clasify --input request.json
```

---

## Migration from semanticAssess / Jev alias

`clasify` replaced `semanticAssess` in a hard cutover. There is no alias.

| Old | New |
|---|---|
| `octocode semanticAssess` | `octocode clasify` |
| `scheme semanticAssess` | `scheme clasify` |
| `next.assess` | `next.clasify` |
| `octocode-semantic-assess` | `octocode-clasify` |
| `tools.enabled: ["semanticAssess"]` | `tools.enabled: ["clasify"]` |
| `DISABLE_TOOLS=semanticAssess` | `DISABLE_TOOLS=clasify` |

Calling `semanticAssess` or `jev` as a tool name fails at admission with an unknown-tool error. Update any operator allowlists, MCP configs, and skill scripts that named the old tool. The credential (`OCTOCODE_CLASSIFICATION_API`), provider (Jev), and configuration schema are unchanged.

The frozen pre-cutover Jev evaluations are preserved under [`.octocode/`](../.octocode/) as historical evidence. Do not use them to construct current calls.

---

*Live schema is always authoritative. Run `npx octocode scheme clasify --compact` before hand-authoring calls.*
