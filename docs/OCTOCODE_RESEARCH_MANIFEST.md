# Octocode Research Manifest

**Abstract.** Agentic code research fails in two symmetric ways: an LLM
reasoning without deterministic backing hallucinates line numbers and
generalizes from one grep; deterministic tools without an LLM directing them
cannot decide what the question even is. This manifest specifies the routing
algorithm that avoids both failure modes for Octocode's 13-tool surface —
grep, AST, LSP, and provider search spanning local disk, GitHub/npm, and a
federated query layer — and treats every load-bearing claim as something to
re-prove, not assert. All of it was re-verified live against the running
toolset on 2026-07-07: every tool exercised, every minification mode
measured, every pagination cursor family followed end-to-end, every
local↔external bridge tested. Where a behavior is config-gated or a known
gap, it is flagged inline rather than smoothed over. §0–§2b build the model;
§3–§8 are the operational algorithm; §9–§11 are the mechanics; §12–§13 are
anti-patterns and a rated self-assessment; §14 places this work against the
2025–2026 context-engineering literature.

**Related work.** This manifest covers *deterministic* retrieval (grep, AST,
LSP, provider index) at per-query granularity. Adjacent art: SWE-agent's
agent-computer-interface theory ([arXiv:2405.15793](https://arxiv.org/abs/2405.15793))
designs tool *interfaces* but no operational routing; aider's
[repo-map](https://aider.chat/docs/repomap.md) does graph-ranked context selection
under a token budget (a whole-context optimization this doc does not attempt);
OpenDev ([arXiv:2603.05344](https://arxiv.org/html/2603.05344v1)) ships anchor-based
retrieval selection and layered LSP without evidence-grading or verification;
tool surveys (e.g. rywalker's code-intelligence comparison, Mar 2026) give
per-*product* guidance where this doc routes per-*query*. AutoWiki-style
generators — Factory AutoWiki, Devin DeepWiki, Google Code Wiki, LangChain
OpenWiki — solve a different problem (continuously-regenerated repo narrative)
and are consumed, not implemented, by this manifest: see §1c. For the retrieval
lanes this manifest deliberately does not use, see §1b. For where this manifest's
own retrieval-layer scope sits against the wider 2025–2026 context-engineering
literature (compaction, memory, sub-agent isolation), see §14.

**Contents.** [0](#0-the-thesis--agentic-research-not-agentic-guessing) The
thesis · [1](#1-the-core-model) Core model · [1b](#1b-the-lanes-this-manifest-does-not-use--embeddings--knowledge-graphs)
Lanes not used · [1c](#1c-existing-wikisdocs-as-a-lead--not-a-lane-not-proof)
Wikis as a lead · [2](#2-tool-matrix) Tool matrix · [2b](#2b-bulk-queries-parallelism-and-triangulation)
Bulk triangulation · [3](#3-the-router-master-decision-tree) The router ·
[4](#4-where-does-the-code-live-local-vs-external-gate) Local/external gate ·
[4b](#4b-matchstring--the-anchor-read-primitive-use-it-by-default) matchString ·
[5](#5-local-algorithm-the-loop-in-full) LOCAL algorithm · [6](#6-external-algorithm-github--npm)
EXTERNAL algorithm · [7](#7-node_modules-first-before-any-external-hop) node_modules-first ·
[8](#8-local--external-bridges) Local↔external bridges · [9](#9-minification--measured-guide)
Minification · [9b](#9b-smart-schema--pay-for-the-contract-only-when-youre-about-to-use-it)
Smart schema · [10](#10-pagination--cursor-families-all-verified) Pagination ·
[11](#11-failure-semantics--recovery) Failure semantics · [12](#12-anti-patterns-each-observed-to-cost-real-round-trips)
Anti-patterns · [13](#13-algorithm-ratings--self-assessment-with-the-gaps-that-limit-it)
Ratings · [14](#14-where-this-sits-in-the-context-engineering-literature)
Literature position · [Appendix](#appendix-tool-agnostic-mapping) Tool-agnostic mapping.

---

## 0. The thesis — agentic research, not agentic guessing

Agentic research works when an LLM's judgment (what to look for, when evidence
is enough, which lane to trust, what "impact" or "unused" even means here) is
paired with tooling that is deterministic and semantic exactly where an LLM is
not: exact line numbers, exhaustive matches, provable call graphs, canonical
continuations. Octocode's tools exist to carry that second half so the agent's
reasoning budget goes to judgment, not bookkeeping. Reasoning without
deterministic backing hallucinates line numbers and "confirms" from one grep;
deterministic tools without an agent directing them can't decide what the
question even is. The local/external/federated loop in §1 is not incidental
plumbing to route around — it *is* the mechanism that makes agentic research
work at all, and every claim below was re-verified live against the running
CLI (`packages/octocode/out/octocode.js`, 2026-07-07) while writing this section.

**The semantic fallback ladder.** When the strongest lane can't answer, the
next lane down still can — this is what makes "I don't know" rare instead of
a dead end: LSP unsupported for a language (§5, Rust `documentSymbols`) → fall
back to `minify:"symbols"` (tree-sitter, language-wide, not LSP-dependent) or
lexical grep; GitHub's provider index returns `providerUnindexed` or a false
zero (§6) → fall back to materializing the path and grepping it locally;
LSP call-hierarchy returns zero callers → that is a capability signal, not an
absence proof (§11) — cross-check lexically before concluding. Verified live
just now: `lspGetSemantics callers` on `executeLspGetSemantics` returned
`incomingCalls: 0` with `dynamicCallsExcluded: true` and the tool's own
warning — *"Zero LSP results after bounded consumer warm-up — still not proof
of unused. Cross-check with a text search"* — and a plain lexical count on the
same symbol immediately found 2 real occurrences in `tools/toolConfig.ts`, a
dispatch table LSP's static call graph cannot see through. Same finding as
§5.5, reproduced fresh, same day, different symbol.

Seven mechanisms make the loop cheap in practice — each re-measured today:

1. **Hints — the tool plans your next call.** Every `search` code row ships a
   prefilled, copy-paste OQL query for the obvious follow-up, not just a path.
   Verified: a text hit for `executeLspGetSemantics` returned
   `next.semantic → {target:"semantics", params:{type:"documentSymbols"}}` and
   `next.fetch → {target:"content", fetch:{content:{match:{text:"executeLspGetSemantics"}}}}`,
   both directly executable. Deeper results add `nextHints[].why` +
   `confidence` — e.g. `"Read the code at this symbol location." confidence:"exact"`
   — so the agent knows *why* a continuation is offered, not just that one exists.
2. **Pagination — cursors are opaque, nothing silently drops.**
   `pagination.nextCharOffset` / `nextPage` are copy-only fields the agent
   never computes (§10); every paginator here is lossless by construction.
3. **Smart schema — pay for what you ask for, not what you might ask for.**
   Schema is tiered and opt-in, measured today: the tool catalog (`tools --json`
   — names + one-liners) is 6,728 chars and is what an agent sees by default;
   one tool's full field-level schema (`tools localSearchCode --scheme`) is
   21,861 chars and is fetched only right before that tool is called raw. Same
   split for OQL: the full contract (`search --scheme`) is 20,372 chars, the
   lean agent-facing guide (`search --scheme --compact`) is 2,153 — **9.5× smaller** —
   and is what should be read first. Full breakdown: §9b.
4. **Smart moves across local/external/federated.** One call —
   `search "coerce" colinhacks/zod/packages --search path` — silently triggered
   the external→local bridge (§8): the repo was cloned to
   `~/.octocode/tmp/clone/colinhacks/zod/...`, and reading the hit came back
   with `"backend": "localGetFileContent"` in the provenance even though the
   question started on GitHub. The agent asked one external question and
   received local-grade evidence with zero manual clone/attach steps. The same
   call also reconfirmed §7's warning live: zod's current default branch is a
   `packages/` monorepo (v3/v4 split), not the flat `src/` layout an agent
   might assume — GitHub's tree is not the installed version's shape.
5. **Smart token management — minify on demand, never by default surprise.**
   Same 1,698-line file, three views, measured today: exact `50,736` chars →
   `standard` (compact) `34,421` (~0.68×) → `symbols` `12,212` (~0.24×). Pull
   the outline first (§9); pay for full bodies only on the lines that matter.
6. **Reasoning fields — state the goal before you fire, honestly scoped.**
   Every query object carries `id`, `mainResearchGoal`, `researchGoal`,
   `reasoning`. Traced live through the source (`executionGuard.ts` →
   `utils.ts` → `error.ts`): `id` is genuinely echoed back in every bulk
   result for correlation, but `mainResearchGoal`/`researchGoal`/`reasoning`
   are accepted and typed, not (yet) echoed back or consumed downstream — the
   internal parameter holding them is literally named `_query` and unused.
   Their real value today is forcing the *agent* to state what it's trying to
   learn and why this specific call advances it before the call fires —
   a discipline against impulsive, ungrounded queries — not a server-side
   feature. Use the discipline; don't oversell what isn't wired up yet.
7. **Multi-angle batching — one call, several angles, not just several
   independent lookups.** The 5-query batch (§2) isn't only for unrelated
   parallel lookups; fire the SAME question through different lanes in one
   call and let disagreement between the angles be the finding. Full
   worked example, verified live today: §2b.

---

## 1. The Core Model

Three research surfaces, one loop:

```
LOCAL      workspace files, node_modules, cloned/materialized repos
           → localSearchCode, localGetFileContent, localViewStructure,
             localFindFiles, localBinaryInspect, lspGetSemantics

EXTERNAL   GitHub (code, trees, files, PRs, commits) and npm
           → ghSearchCode, ghGetFileContent, ghViewRepoStructure,
             ghSearchRepos, ghHistoryResearch, npmSearch

FEDERATED  one typed query planned across both
           → oqlSearch (run `search --scheme` for the full contract)
```

Evidence has grades. Treat them differently:

| Grade | Source | Trust |
|---|---|---|
| **semantic** | LSP (definitions, references, callers) | Proven identity — but scoped to the language project; blind to scripts, re-exports-as-text, strings, docs |
| **structural** | AST match with metavar ranges | Proven shape — complete-node semantics, exact captures |
| **lexical** | ripgrep text/regex (rows come pre-classified: `kind: declaration/callsite/import/comment` + scoreHint) | Total coverage — sees everything, proves nothing about identity |
| **provider** | GitHub search index | Weakest: default-branch only, unindexed/archived repos return false zeros. `providerSemanticsApproximate` — no line numbers |

**The one non-negotiable rule (verified, §5 step 5):** never conclude from a single
grade. Semantic and lexical lanes each miss things the other catches. In live testing,
LSP `callers` of a function missed a diverged duplicate implementation in a `.mjs`
script and a barrel re-export — one grep caught both; conversely grep alone cannot
distinguish a call from a comment. The *disagreement between lanes is itself a finding*.

---

## 1b. The lanes this manifest does NOT use — embeddings & knowledge graphs

Two retrieval families exist in the field that this toolset deliberately omits;
know when they would beat you, and say so rather than pretending they don't exist.

**Indexed/semantic retrieval (embeddings, vector search).** Tools like
claude-context (embeddings+BM25) and grepai index the repo and answer *fuzzy
concept queries* ("where is retry logic handled?") without exact terms. If it
existed here it would slot into §1's table as: **indexed** — high recall on
fuzzy concepts across huge unfamiliar corpora; proves nothing about identity;
adds index-staleness risk (results reflect the index, not the working tree).
Vendor token-reduction claims (40–97%) are mostly self-reported — weigh accordingly.

**When the deterministic loop still wins:** you hold *any* concrete handle
(identifier, filename, error string, code shape) — the router (§3) reaches proof-grade
evidence with zero index setup and zero staleness. **When an index wins:** purely
conceptual queries over very large unfamiliar codebases where synonym-regex grep
fans out too wide. This manifest's fallback for that case is orientation
(§3 "nothing" branch: tree + hotspot map + symbols skeletons) — slower than a good
index, but always fresh and evidence-graded.

**Knowledge graphs / code graphs.** Graph tools precompute blast-radius and
impact edges. Here that job is done at query time by LSP call hierarchy +
the mandatory cross-check (§5.3–5.5) — verified correct but per-query, not
precomputed. For repo-wide impact sweeps at monorepo scale (>100k files),
expect a precomputed graph to be materially cheaper; this loop has not been
validated at that scale.

**Positioning in one line:** deterministic-first, evidence-graded, zero-index —
and honest that fuzzy-concept recall at mega-scale is the one job where an
indexed lane is the better entry point (the router still applies for the
verification steps that follow).

---

## 1c. Existing wikis/docs as a lead — not a lane, not proof

AutoWiki-style tools (Factory AutoWiki, Devin DeepWiki, Google Code Wiki,
LangChain OpenWiki) generate and continuously refresh repo-level narrative
docs — architecture summaries, module maps, sometimes a chat layer — synced
via `git diff` on push. When a repo already has one (`ARCHITECTURE.md`,
`droid-wiki/`, `openwiki/`, `.devin/wiki.json`, a GitHub Wiki tab, or a
DeepWiki/Code Wiki page), it is a **fast orientation lead**, not a new evidence
grade: treat its claims exactly like a provider snippet — useful for naming
entry points and shaping the first query, worthless for "impact is X" or
"unused" without the §5.5 cross-check. Two failure modes to watch for: the doc
describes an old shape of the code (staleness — worse than no doc, because it's
confidently wrong), and the doc's own confidence reads as proof it is not.
This manifest does not generate or maintain such docs (out of scope — that is
the cited tools' job); it only consumes them as a router input (§3).

---

## 2. Tool Matrix

| Tool | Surface | Role | Reach for it when |
|---|---|---|---|
| `localSearchCode` | local | text/regex/AST search, count modes, ranked | any local content question — the workhorse |
| `localGetFileContent` | local | read file / matchString slices / line ranges | reading after you have coordinates |
| `localViewStructure` | local | directory tree | orientation in an unfamiliar dir |
| `localFindFiles` | local | find by name/size/time/permissions metadata | the constraint is *about the file*, not in it |
| `localBinaryInspect` | local | archives, compressed streams, native binaries | non-text artifacts (list → extract; inspect; strings) |
| `lspGetSemantics` | local | definitions, references, callers/callees, hover, symbols, types | proving identity and impact |
| `ghSearchCode` | external | GitHub code/path search | locating code across repos you don't have |
| `ghGetFileContent` | external | read GitHub file (slices/ranges/symbols); `type:"directory"` materializes a subtree | reading remote files; bridging remote→local |
| `ghViewRepoStructure` | external | GitHub tree browse | orienting in a remote repo |
| `ghSearchRepos` | external | repo discovery | finding candidate repos/prior art |
| `ghHistoryResearch` | external | PR search + PR deep-read + commit history | archaeology: why did this change |
| `npmSearch` | external | package → source repo (+ `repositoryDirectory`) | resolving a dependency to its home |
| `oqlSearch` | both | typed federated query; research/graph/diff/artifacts targets | multi-predicate queries, remote+local in one plan |
| `ghCloneRepo` | bridge | full/sparse clone (**gated: `ENABLE_CLONE=true`**) | whole-repo local analysis |

Bulk: every tool takes up to 5 parallel queries per call with per-query `id`.
Batch independent probes into ONE call — it is the cheapest parallelism you have.
That is only half the value of the batch — see §2b for using it to triangulate
one question instead of just parallelizing unrelated ones.

---

## 2b. Bulk queries: parallelism AND triangulation

The 5-query batch is usually read as a speed feature (5 independent lookups,
1 round-trip). It is also a **correctness** feature: fire the same question
through different lanes — lexical, structural with one shape, structural with
another shape — in a single call, and treat any disagreement between the
angles as the actual finding, not noise to average away.

Worked example, run live 2026-07-07 as one `localSearchCode` bulk call
against this repo's own `packages/octocode-awareness/src/db.ts`, asking one
question three ways — *"is the `_db` module singleton ever reassigned or
read-guarded outside `connectDb`?"*:

| Angle | Query | Result |
|---|---|---|
| lexical | `keywords:"_db\s*="` (regex) | 1 hit — the assignment inside `connectDb` |
| structural (assignment shape) | `pattern:"_db = $VAL"` | 1 hit, AST-proven — same assignment, now identity-confirmed |
| structural (guard shape, guessed) | `pattern:"if (!_db) { $$$BODY }"` | **0 hits** |

The third angle's zero looked like "no read-guard exists." It was wrong about
the *shape*, not the *fact*: a follow-up lexical angle (`matchString:"!_db"`)
immediately found `if (!_db) throw new Error('Database not connected...')` in
`getDb()` — a real guard, just brace-less, so the guessed AST pattern
(`{ $$$BODY }`) could not match a single-statement `if`. One angle alone would
have closed the question wrong ("no guard"); three-plus angles in one batch
turned the mismatch itself into a finding (§11: structural 0 matches is a
shape signal, not an absence proof) instead of a silently wrong conclusion.

**When to spend a batch slot on a second angle instead of a second file:**
whenever the question is a claim ("this is unused", "this is never
reassigned", "this is always guarded") rather than a location lookup. Location
lookups want 5 independent files/paths; claims want 2–3 angles on the *same*
target — lexical for total coverage, structural for shape-proof, and (per
§5.5) semantic when a callable identity is in question.

---

## 3. THE ROUTER (master decision tree)

Route by **what you already hold**, not by a fixed pipeline order.
Running grep first when you hold a symbol name wastes a hop; running LSP first
when you hold only a concept cannot work at all.

```
WHAT DO I HOLD?
│
├─ Nothing, AND a wiki/doc artifact already exists (§1c: ARCHITECTURE.md,
│  droid-wiki/, openwiki/, .devin/wiki.json, GitHub Wiki, DeepWiki/Code Wiki)
│    → read it first for orientation and named entry points ← a lead, not proof
│    → re-enter router (§3) to verify any specific claim before relying on it
│
├─ Nothing (unfamiliar codebase, no wiki/doc artifact)
│    → localViewStructure (tree, maxDepth 1-2)
│    → localSearchCode countMatchesPerFile on the domain term   ← hotspot map, 1 call
│    → then re-enter router with what you learned
│
├─ A concept / behavior (words, no identifier)
│    → localSearchCode with synonym regex: "halfLife|half_life|HALF_LIFE"
│    → top file → localGetFileContent minify:"symbols"          ← the anchor sheet
│
├─ An identifier (function/class/const name)
│    → lspGetSemantics workspaceSymbol                          ← skip grep for locating
│    → callers/callees (callables) or references groupByFile (everything else)
│
├─ A code shape ("all X calls that do Y")
│    → localSearchCode mode:"structural" with a rule            ← metavars = typed extraction
│
├─ A package name
│    → node_modules FIRST (§7), npmSearch only to find the repo
│
├─ A "why" / history question
│    → ghHistoryResearch (PRs: keywords+match:["title"], concise:true; commits: owner/repo/path)
│
└─ A binary / archive / huge artifact
     → localBinaryInspect (list before extract; inspect for metadata; strings for leads)
```

---

## 4. Where does the code live? (LOCAL vs EXTERNAL gate)

Before any external call, ask: **is the code already on disk?**

```
Is it my workspace code?                → LOCAL tools. Done.
Is it a dependency I have installed?    → node_modules IS local. Search it (§7). Done.
Is it a repo I already materialized?    → the localPath from a previous fetch/clone. Done.
Only then                               → EXTERNAL (GitHub/npm), and consider
                                          materializing early if >2 reads are coming (§8).
```

Verified: `localSearchCode` over `node_modules/zod` (718 files, `excludeDir: []`,
`noIgnore: true`) found the exact installed source in 16 ms — with the version that
actually runs, which GitHub's default branch is NOT guaranteed to be.

---

## 4b. matchString — the anchor-read primitive (use it by default)

`matchString` on `localGetFileContent` / `ghGetFileContent` is the highest-leverage
read mode in the toolset. Instead of guessing line ranges or paging a whole file,
you hand it the string (or regex) you care about and get back only the relevant
slices — **plus machine-usable anchors for the next step**. Verified on both
surfaces, on workspace files, and on clone-materialized files:

```
localGetFileContent / ghGetFileContent
  matchString: "sanitizeStructuredContent"       ← literal, case-insensitive default
  matchString: "export function (decayScore|decayComponents)"
    + matchStringIsRegex: true                    ← regex anchors
  contextLines: N                                 ← raise to capture whole bodies
```

What you get (all verified):
- **Merged slices, not N reads**: 7 occurrences collapsed into 4 minimal slices
  with `... [N lines omitted] ...` separators — one call replaced seven.
  Reconfirmed live 2026-07-07 on this repo's own `db.ts`: `matchString:"db.exec"`
  (8 real occurrences) came back as merged slices with real
  `... [9 lines omitted] ...` / `... [177 lines omitted] ...` separators, not
  8 separate reads.
- **`matchRanges[]`**: exact `{start,end}` line ranges per slice — feed them
  straight into `startLine/endLine` follow-ups or LSP `lineHint`.
- **The warning text names your anchors**: "Found 7 occurrences on lines 256, 261,
  269... — these lines are lineHint anchors for lspGetSemantics." The tool is
  literally handing you the next call.
- **Regex mode fetches multiple related definitions in one read** (two function
  defs + full signatures via one regex, 6 context lines each).
- **Works identically remote**: same anchors from a GitHub file — so you can go
  ghSearchCode (which file) → ghGetFileContent matchString (which lines) →
  materialize → LSP at those lines, without ever reading a full file.
- **The federated `search` shorthand goes further than a warning string**: a
  code-search row's `next` object is a directly-executable OQL query object,
  not prose — `next.fetch` and `next.semantic` came back pre-populated with
  `from`, `target`, and `params` filled in (verified 2026-07-07, §0). Some
  results add `nextHints[].why` + `confidence` explaining *why* that
  continuation was offered, e.g. `"Read the code at this symbol location."`
  / `confidence:"exact"` — reasoning support, not just a pointer.

Default read policy: **matchString first, line ranges second, fullContent last**
(small files only). If you know *what* you're looking for but not *where*, this
is always the cheapest correct read. Related but different: `ghHistoryResearch
matchString` filters PR patches/comments to matching sections — same idea, diff
surface; `localBinaryInspect matchString` does it for extracted/decompressed
streams with `matchStringContextLines`.

---

## 5. LOCAL algorithm (the loop, in full)

```
0. ORIENT     (skip if you know the area)
   localViewStructure         — shape of the directory
   countMatchesPerFile        — which files carry the concern (measured: instant map,
                                reconfirmed 2026-07-07 with keywords:"session" over
                                octocode-awareness/src → sessions.ts:60, pi-hooks.ts:49,
                                intents.ts:33, db.ts:25, one call, no snippets read)

1. LOCATE     (router entry, §3)

2. MAP        localGetFileContent minify:"symbols" on the winning file.
              Reconfirmed 2026-07-07 on a real 1,698-line file (§9): exact
              50,736 chars → symbols skeleton 12,212 (~0.24×), every signature
              kept with a line-gutter number. Those line numbers are ready LSP
              anchors. This one call often answers concept questions outright.

3. PROVE      lspGetSemantics from a real anchor (grep line / symbols line):
              - callers/callees/callHierarchy for callables (impact analysis)
              - references includeDeclaration:false, groupByFile:true for the rest
              - lineHint SELF-CORRECTS (verified: passed 261, resolved to 263 and
                reported foundAtLine; reconfirmed 2026-07-07 on a different
                symbol — passed 260, resolved to 264, see §0) — but never guess
                it from nothing
              - READ the completeness block: truncatedByDepth, dynamicCallsExcluded,
                stdlibCallsExcluded, failedRequestCount tell you what you did NOT see

4. EXTRACT    localSearchCode mode:"structural" when you need the complete node or
              typed captures. Reconfirmed live 2026-07-07 on this repo's own code
              (`packages/octocode-awareness/src/db.ts`): pattern `db.exec($$$SQL)`
              matched 8 real calls and captured a 6,835-character multiline
              `CREATE TABLE` statement (line 82) whole, as one `$SQL` metavar —
              grep only sees one line of a many-line template literal. Isolating
              just the `ALTER TABLE` call (line 358) out of those 8 needed the
              pattern **nested inside** the rule, not passed alongside it:
              `rule: "pattern: db.exec($$$SQL)\nhas:\n  regex: ALTER TABLE\n  stopBy: end"`
              — passing `pattern` and `rule` as sibling fields is rejected
              (`"provide either pattern or rule, not both"`); this returned
              exactly the 1 matching row.
              Rules of the mode:
              - a `pattern` must match a COMPLETE AST node (body, return type, all
                required syntax). Partial shape → 0 matches (pattern queries get a
                guidance warning; rule queries currently do not).
              - method calls ≠ plain calls: reconfirmed live on the same file —
                bare `exec($$$A)` = 0 matches; `$RECV.exec($$$A)` = 8 matches,
                identical code. `foo($$$A)` will not match `x.foo($$$A)`.
              - for partial/relational matches use a YAML rule (kind/has/inside/not/any,
                stopBy: end) with `pattern` nested inside the rule string, not as a
                sibling field. Bare rule YAML and `rule:`-wrapped are both accepted
                (engine ≥ 2026-07-07; older engines require the `rule:` wrapper).
              - `$$$LIST` captures currently include comma separators as elements — filter.

5. CROSS-CHECK  (non-negotiable before "impact is X" / "unused" / "only used in Y")
              One package-wide grep of the symbol INCLUDING tests/scripts/configs.
              Diff lexical hits vs semantic hits. Every lexical hit LSP didn't report
              is a finding: re-export, shadow copy, string/SQL/config reference, doc.
              (Verified: this exact step exposed a diverged duplicate scorer in a
              skills/*.mjs script that LSP callers could not see. Reconfirmed
              2026-07-07 on a different symbol: `callHierarchy` reported 0
              incoming calls for `executeLspGetSemantics` — dynamicCallsExcluded,
              a dispatch-table registration — while a plain lexical count found
              2 real occurrences in `toolConfig.ts` in the same call. See §0.)

6. READ       matchString first (§4b) — merged slices + matchRanges anchors;
              startLine/endLine when you already hold exact coordinates.
              minify:"none" whenever you will quote, diff, or edit — lossy modes
              rewrite whitespace/quotes (verified: standard mode rewrites '..'→`..`).
```

LSP capability notes (verified): pull `diagnostic` may be unsupported (server pushes
instead — the tool says so rather than returning a fake empty); native `documentSymbols`
is JS/TS-only — for Rust and others use `localGetFileContent minify:"symbols"` which is
tree-sitter based and language-wide. `serverUnavailable`/`unsupported` = capability
absence, NOT "no usage".

---

## 6. EXTERNAL algorithm (GitHub + npm)

```
0. RESOLVE    package name → npmSearch → owner/repo + repositoryDirectory.
              Skip if you already know owner/repo.

1. ORIENT     ghViewRepoStructure at repositoryDirectory (or root).
              resolvedBranch in the result is the branch every follow-up should use.

2. LOCATE     ghSearchCode:
              - match:"path" first when a filename fragment is known (far cheaper)
              - keywords are ANDed; alternatives go in separate bulk queries
              - scope hard: owner+repo, path prefix, extension/language
              - concise:true until snippets/matchIndices actually matter

3. READ       ghGetFileContent:
              - matchString anchor → merged slices + matchRanges (line anchors)
              - minify:"symbols" works on GitHub files too (verified: 12.7 KB file →
                3.6 KB line-gutter skeleton; reconfirmed 2026-07-07 on the SAME
                file read both locally and from GitHub — local exact 50,736 →
                symbols 12,212 (~0.24×); GitHub exact 50,371 → symbols 11,963
                (~0.237×). Near-identical ratio, near-identical byte counts —
                minification is not a local-only shortcut.) — orient before
                pulling bodies
              - startLine/endLine for known ranges; fullContent only for small files

4. WHY        ghHistoryResearch:
              - PR triage: keywords + match:["title"] + concise:true, then prNumber +
                content selectors (body/patches mode:"selected"/comments) for depth
              - archaeology: state:"merged" sort:"created" order:"asc"
              - commit lane: type:"commits" owner/repo/path (trailing "/" = subtree)

5. ESCALATE   the moment you need AST, LSP, multi-file grep, or >2 more reads:
              materialize and go local (§8).
```

**GitHub index blind spots (all verified/known):** default-branch-only; archived
repos return zero code hits; renamed repos redirect for content APIs but silently
fail for search; the code-search API has an upstream deprecation (~Sep 2026).
Therefore: **an empty ghSearchCode is NOT absence.** Verify with
`ghViewRepoStructure` (does the path exist?) or `ghGetFileContent`, or materialize
and grep locally. Never report "X does not exist in repo Y" from provider search alone.

---

## 7. node_modules FIRST (before any external hop)

The installed dependency is (a) already on disk, (b) the exact version that runs,
(c) searchable in milliseconds. GitHub shows you a default branch that may be
newer, older, or restructured.

```
Question about a dependency's behavior?
  1. localViewStructure node_modules/<pkg>          — what shipped (dist? src? types?)
  2. localSearchCode path:node_modules/<pkg>        — MUST set excludeDir: []
       + noIgnore: true                              (defaults skip node_modules)
  3. localGetFileContent on the hit                 — .d.ts and shipped src are gold
  4. LSP hover/definition often resolves INTO node_modules types for free
       when anchored from your own importing file
  5. ONLY IF the answer isn't in the shipped artifact (needs git history, tests,
     unshipped sources): npmSearch → repo → §6 external loop
```

Gotchas (verified): `excludeDir: []` is mandatory — the default exclusion list
silently skips node_modules and a "no matches" there means "didn't look".
Watch for dual hits (src/ + dist/ in the same package) — prefer the one your
resolver actually loads when semantics matter.

**Nuance found while reconfirming this live (2026-07-07), worth knowing exactly:**
pointing `path` directly AT `node_modules/<pkg>` reaches it fine with no
`excludeDir` override at all — the default exclusion filters directory names
encountered *while walking*, not the root path you hand it. But searching a
parent path (e.g. `.`) with an explicit `include: "node_modules/**"` glob is
still fully blocked by default (`zeroMatches`, proven with a real query against
this repo) — `include` does not override `excludeDir`; only clearing
`excludeDir` (`""`/`[]`) + `noIgnore:true` does, and once cleared the same
query reached real files (`node_modules/.ignored/eslint/node_modules/ajv/lib/dot/coerce.def`).
So: scoping `path` straight into a known package is always safe; scoping a
wider search and hoping `include` reaches into `node_modules` is not.

Reconfirmed live 2026-07-07 (§0): a path-scoped search into `colinhacks/zod`'s
current default branch showed a `packages/zod/src/v3` + `v4` monorepo split —
not the flat `src/` layout an agent might remember or assume from an older
snapshot. The installed `node_modules/zod` is unaffected by any of that
upstream restructuring; it is still the one true source for "what actually runs."

---

## 8. LOCAL ↔ EXTERNAL bridges

### External → Local (materialize, then analyze) — VERIFIED at all three depths

All gated by `ENABLE_CLONE=true` (off → typed error saying exactly that).
Each depth was exercised end-to-end and the full §5 loop confirmed on the result:

| Depth | Call | What lands on disk (verified) | Use when |
|---|---|---|---|
| **file** | `ghCloneRepo` + `sparsePath: "path/to/file.ts"` | sparse checkout: the file's subtree **plus repo-root files** (README, package.json, configs — git sparse-checkout keeps root); `complete:false` flagged | one file needs repeated matchString/LSP reads |
| **tree** | `ghGetFileContent type:"directory"` | just that subtree under `~/.octocode/tmp/tree/<owner>/<repo>/<branch>/...`, with `commitSha`, per-reason skip accounting (`oversized`/`binary`/`fileLimit`/...) and disclosed limits (50 files, 5 MB total, 300 KB/file); partiality warning when limits bite | analyzing one directory |
| **repo** | `ghCloneRepo` (no sparsePath) | full shallow clone, `complete:true`, 24h-cached (`forceRefresh` to bust) | repo-wide grep/AST/LSP, dead-code, reachability |

Every result carries `localPath` + prefilled `next.localSearch` / `next.viewStructure`.
OQL reaches the same machinery via `materialize: "auto"/"required"` or a row's
`next.materialize`.

**Verified post-bridge:** structural AST rules, native LSP `documentSymbols`, and
matchString anchor reads all ran unmodified on the materialized paths — remote code
becomes fully local-grade evidence after ONE bridge call.

Reconfirmed live 2026-07-07 (§0), and worth noticing because it happened
*without an explicit clone step*: a plain `search --search path` query scoped
to a GitHub path silently triggered this exact bridge, and the file read that
followed reported `"backend": "localGetFileContent"` in its provenance —
the tool chose to materialize rather than answer from the provider index,
and the agent never had to ask for that choice explicitly.

**When to materialize:** you need AST/structural, LSP, multi-file regex, or you're
about to make a 3rd+ read call into the same remote area.

**Honesty caveats (from the tool itself, respect them):** tree materialization is
bounded — check `skipped` counts before any "not present" claim on a materialized
tree; sparse clones are `complete:false` by definition; prefer depth=repo before
repo-wide reachability/dead-code conclusions (the warning says exactly this).

### Local → External (context enrichment)

- symbol came from a dependency → §7 first, then npmSearch → repo → docs/tests/history
- "why is this code like this" → `ghHistoryResearch type:"commits"` on the file path,
  then the PR behind the commit (`reviewMode:"full"` for the whole story)
- "has someone solved this" → `ghSearchRepos` (concise triage) → §6 on candidates

### Federated in one shot (OQL)

`oqlSearch from:{kind:"github",owner,repo}` plans provider search + optional
materialization for you. Verified behaviors to rely on:
- GitHub code rows come back `proofGrade:"text"` with `evidence.answerReady:false`
  and a `providerSemanticsApproximate` diagnostic — that is NORMAL, not failure;
  each row carries a prefilled `next.fetch` to upgrade to exact content
- `target:"research"` page 1 = summary counts; packets from page 2+; `next.graph`
  upgrades rows to proofStatus (confirmed-by-lsp / conflicting-evidence / …)
- zero rows + `providerUnindexed` ≠ absence — follow `next.materialize`
- run `search --scheme` before authoring nontrivial OQL; `--explain` to see routing

---

## 9. Minification — measured guide

| Mode | What it does | Measured | Use when | Never when |
|---|---|---|---|---|
| `none` | verbatim bytes | 1.0× | quoting, diffing, editing, regex-sensitive reads | — |
| `standard` (default) | strips comments/blanks, compacts whitespace, may rewrite quote style | ~0.58× chars on real TS (35,260 → 20,524) | general reading | anything you'll quote verbatim — it is LOSSY (verified: rewrites `'x'` → `` `x` ``, drops comments) |
| `symbols` | signatures + constants outline with `NNN\|` line gutter | ~0.10–0.28× (956 lines → skeleton; 12.7 KB → 3.6 KB on GitHub) | orientation, building an anchor sheet, API surface review | reading logic bodies |

Works identically on local and GitHub files (both verified). The `symbols` gutter
numbers are valid `lineHint`/`startLine` anchors.

Search-side equivalents: `concise:true` (gh tools) for triage lists;
`filesOnly`/`countMatchesPerFile` (local) for maps; `format:"compact"`/`groupByFile`
(LSP) for wide result sets; `content.patches mode:"selected"` + `ranges` (PRs) to
avoid whole-diff pulls.

Reconfirmed live 2026-07-07 on a 1,698-line logic file (real bytes, not a
toy fixture): `exact` 50,736 chars → `standard` 34,421 (~0.68×) →
`symbols` 12,212 (~0.24×) — same shape as the original measurement, different
file, different day.

---

## 9b. Smart schema — pay for the contract only when you're about to use it

Schemas are large by nature (every field, type, bound, default) and an agent
does not need most of them most of the time. The design answer is tiering,
not omission — measured live 2026-07-07:

| What you ask for | Size | When |
|---|---|---|
| `tools --json` — tool catalog, names + one-liners | 6,728 chars | default orientation: which of the 13 tools is this? |
| `tools <name> --scheme` — one tool's full field-level schema | 21,861 chars (`localSearchCode`) | right before calling that tool raw — never guess a field |
| `search --scheme --compact` — lean OQL agent guide (source/target/recipes) | 2,153 chars | read this first for any non-trivial OQL query |
| `search --scheme` — full OQL contract (every target/predicate/param) | 20,372 chars | only when the compact guide didn't resolve an edge case |

The compact OQL guide is **~9.5× smaller** than the full contract and answers
the routing question ("which target, which source") that blocks most queries
— read it before the full contract, not instead of it when something is
still ambiguous. The same logic applies one level up: read the tool catalog
before any single tool's schema, and read a single tool's schema before ever
calling it with guessed field names (§12 anti-pattern: never guess a field
that a one-line schema read would have shown you).

---

## 10. Pagination — cursor families (all verified)

**Golden rule: cursors are OPAQUE. Copy them from the response
(`pagination.nextCharOffset`, `nextPage`, `next.*` prefilled queries) — never compute
your own.** Every paginator here is lossless; nothing is silently dropped.

| Family | Fields | Tools | Verified behavior |
|---|---|---|---|
| Char window (file) | `charOffset`/`charLength` → `nextCharOffset`, `isPartial` | local/gh GetFileContent, binary extract/decompress | page 2 resumed mid-token exactly at offset 2000; `next.continueChars` was copy-paste ready. Reconfirmed 2026-07-07: `charLength:5000` on a real file returned `pagination.hasMore:true` plus a ready `next.charRange` query pre-filled with `charOffset:5310` — and a `nextHints.why:"Read the next content window."` explaining the offer. |
| Result page | `page` → `hasMore`/`nextPage` | all search tools, structure tools | gh code search page 2 returned rows 6–10 of 26 with `reported`/`reachable`/`capped` breakdown |
| Per-file match page | `matchPage` + `maxMatchesPerFile` | localSearchCode | walks a noisy file without re-fetching others |
| List pages | `itemsPerPage` + `page`; `filePage`/`commentPage`/`commitPage` | LSP lists, PR content surfaces | documentSymbols 23 symbols paged 15+8 |
| Byte scan | `scanOffset` → `nextScanOffset`; `entryPageNumber` | binary strings / archive lists | for artifacts, not text |
| Response window | `responseCharLength`/`responseCharOffset` | EVERY tool (outermost) | wraps the whole bulk response; advance only on `hasMore`. Wrinkle: on multi-query bulk repo-search, prefer bigger `responseCharLength` or per-query pages over advancing this offset |

Budget levers, cheapest first: tighter scope (path/owner/repo/langType) → leaner mode
(concise/symbols/filesOnly/counts) → smaller pages (`maxFiles`, `maxMatchesPerFile`,
`limit`) → THEN paginate what's left. GitHub `matchIndices` are snippet offsets, not
line numbers — get lines from `ghGetFileContent matchString`.

---

## 11. Failure semantics & recovery

| Signal | Meaning | Move |
|---|---|---|
| `status:"empty"` + stats (`filesSearched`, `bytesSearched`) | proven negative *for that scope* | widen scope / synonyms / drop filters; only then conclude — and quote the stats |
| `status:"error"` + `errorCode` + hint | typed failure; hint names the recovery tool (verified: bad path → "Verify the path exists using localFindFiles") | follow the hint; do not retry verbatim |
| structural 0 matches | usually an incomplete pattern, NOT absence | add `$$$BODY`/return type, or switch to a rule; check the guidance warning (live example, §2b: `if (!_db) { $$$BODY }` → 0 hits on a real, brace-less `if (!_db) throw ...` guard) |
| LSP `serverUnavailable`/`unsupported` | capability absence, NOT "no usage" | fall back to grep / symbols view |
| LSP `completeness.complete:false` | results truncated by depth/dynamic-call exclusion | deepen or supplement with grep before claiming full impact |
| gh empty / `providerUnindexed` | index blind spot, NOT absence | verify path exists → materialize → grep locally |
| `resolvedBranch` ≠ requested | ref fell back to default branch | re-check which branch you're actually reading |
| `warnings[]` | redaction, fallback engine, pre-filter skips, pagination notes | read them — they change what the result means |

`Pre-filter skipped parsing N file(s) (literal anchor absent)` on structural results
is an optimization disclosure, not data loss: those files could not contain the
literal anchor.

---

## 12. Anti-patterns (each observed to cost real round-trips)

1. **Fixed pipeline instead of routing.** grep→AST→LSP is not a law; enter where
   your knowledge already points (§3).
2. **Concluding impact from one evidence lane.** The lexical∩semantic diff is the
   deliverable, not a nicety (§5.5).
3. **Trusting GitHub search zeros.** Default-branch-only + unindexed repos =
   false absence machine (§6).
4. **Reading before mapping.** `minify:"symbols"` first; bodies only for the slices
   that matter (§5.2, §9).
5. **Skipping node_modules.** You debug the installed version, not the default branch (§7).
6. **Guessing lineHint / computing charOffset.** Anchors come from prior results;
   cursors come from `pagination.*` (§5.3, §10).
7. **Staying remote too long.** 3+ reads into one remote area ⇒ materialize (§8).
8. **Ignoring `next.*` hints.** They are prefilled, correct continuation queries —
   the tool has already planned your next call (verified everywhere: grep→LSP anchor
   handoff even self-corrects the line).
9. **quoting from `minify:"standard"` output.** It rewrites quotes and strips
   comments; quote only from `minify:"none"` (§9).
10. **Serial single queries.** Up to 5 queries per call, per tool. Batch.
11. **Calling a raw tool with a guessed field name instead of reading its schema
    first.** The catalog (6.7 KB) and OQL compact guide (2.2 KB) are cheap;
    a wrong field costs a full extra round-trip and, worse, a silently-ignored
    parameter (§9b).
12. **Spending a claim on one angle instead of one batch on several.** "Is X
    unused / always guarded / never reassigned" is a claim, not a location
    lookup — one structural pattern guessing the wrong shape returns a clean
    0 that reads exactly like a proven negative (§2b live example: a real
    brace-less guard, invisible to one guessed pattern, obvious to a second
    lexical angle in the same batch).

---

## 13. Algorithm ratings — self-assessment, with the gaps that limit it

Self-reported, from the same session that verified the mechanisms above —
read the ratings as a summary of the evidence already shown, not a new claim:

| Algorithm | Rating | Why / what holds it back |
|---|---|---|
| **LOCAL loop (§5)** | **9.5/10** | Full evidence stack (lexical + structural + semantic + binary) with self-correcting anchors, merged matchString slices, honest completeness metadata, and typed recovery on every failure. Held back only by: the lexical∩semantic cross-check being agent discipline rather than a tool-emitted delta, and per-language LSP gaps (Rust documentSymbols, pull diagnostics). |
| **EXTERNAL/GitHub loop (§6)** | **8.5/10** | Orient→search→matchString-read→history is strong, and symbols/matchString work identically remote. Capped by provider physics the tool can't fix: default-branch-only index, archived/renamed blind spots, no remote AST/LSP. The verified clone bridge is the mitigation — escalating early effectively lifts this to local-grade. |
| **NPM / node_modules-first (§7)** | **9/10** | The installed-version-is-ground-truth rule is cheap, fast (16 ms across 718 files), and correct; npmSearch resolves package→repo→`repositoryDirectory` in one call. Held back by the `excludeDir: []` footgun (a forgotten default silently skips node_modules) and src/dist dual-hit ambiguity. |
| **Bridge: external→local (§8)** | **9.5/10** | One call converts remote code to full local-grade evidence at any of three depths, with per-file skip accounting and self-describing partiality. The only friction: config-gated (`ENABLE_CLONE`), and tree-depth limits require reading the `skipped` counts. |

Weighted overall (internal): **~9/10** — the routed, dual-lane, materialize-early
algorithm is production-grade; the residual points live in provider index limits
and the manual cross-check step.

**Standing gaps** (self-critique, not scored — these are the honest limits of
the internal rating above, restated so they can't be missed): no embeddings/KG
lane (addressed as a position statement in §1b, not as tooling); verification
above covers deterministic mechanisms, not task-success rates — **open: a
task-level A/B benchmark of this routed loop against a grep-only baseline
does not exist** (§14 repeats this rather than re-scoring it); scale beyond
~100k files is untested; and the lexical∩semantic cross-check (§5.5) is agent
discipline enforced by this document, not a delta the tools compute and
emit themselves. What differentiates this algorithm from a generic
grep→AST→LSP pipeline is route-by-what-you-hold (§3), node_modules-first
(§7), typed failure semantics (§11), and anchor-passing ergonomics (§4b) —
but "differentiated" is a design claim, not a measured one; the gaps above
are what would need to close before it could become one.

---

## 14. Where this sits in the context-engineering literature

Since late 2025, "context engineering" — curating the finite token budget an LLM
sees per turn — has converged into a named discipline with a shared vocabulary
(Anthropic, [Sep 2025](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents);
Sourcegraph, [2026](https://sourcegraph.com/blog/context-engineering)). That
literature splits the problem into four layers: instructions, retrieval,
memory, and tools. This manifest is entirely about one of those four —
**retrieval** — and about the tool-provider side of it specifically, not the
agent-harness side. Naming that boundary precisely, instead of implying this
manifest covers ground it doesn't, is the point of this section.

**Where the manifest's mechanisms match the field's best-known patterns:**

| Field pattern | Source | This manifest's answer | Verdict |
|---|---|---|---|
| Just-in-time retrieval — hold a lightweight identifier, load content on demand instead of pre-loading it | Anthropic | `matchString` anchors, `next.fetch`/`next.semantic` (§4b) — the toolset never had a pre-load path to begin with | Matches by construction |
| Memory pointers — a short reference token stands in for content that can be re-fetched | StackOne | Materialized `localPath` + `next.materialize` (§8); OQL rows carry the same pattern | Matches |
| Built-in filters as the pragmatic middle ground for tool *providers* (vs. sandboxed code-mode, which StackOne calls "heavy... most won't build it") | StackOne | `countMatchesPerFile`, `filesOnly`, `discovery`, structural metavars, `concise` (§9, §12) | Matches — independent validation of an existing design choice, not self-assessment |
| Tiered/on-demand schema so tool definitions don't burn the budget up front | StackOne ("Tool Definition Catch-22") | §9b: 6.7 KB catalog vs. 21.9 KB per-tool schema; 2.2 KB compact OQL guide vs. 20.4 KB full contract | Matches — same discipline applied at the field level since the tool count (13) never grew large enough to need discovery-by-search |
| Pre-flight cost awareness — let the agent see or estimate cost before committing | StackOne ("dry-run... their survival becomes their responsibility") | `search --dry-run` plans an OQL query without executing it; every paginated response carries `estimatedTokens` with an actionable warning above 30k/50k tokens (`utils/pagination/{core,hints}.ts`) | Matches — shipped, not aspirational |
| Structural/semantic code intelligence beats plain text retrieval for coding agents, measured | Sourcegraph (CodeScaleBench: file recall 0.127→0.277, P@5 0.140→0.478, F1@5 0.099→0.262 with an MCP code-graph layer vs. grep-only) | §1/§5 evidence grades (semantic > structural > lexical > provider) argue the same ordering qualitatively | Same conclusion, weaker proof: this manifest has never run the equivalent task-level A/B (§13 already lists this as an open item — see below, not restated as new) |

**Where this manifest doesn't compete, on purpose:**

Compaction (Anthropic, the Claude context-engineering cookbook, and
LangChain's Deep Agents SDK all treat LLM-driven summarization of an agent's
own conversation history as one of 2–3 mandatory levers for long-horizon
tasks) has no equivalent here, and shouldn't: this manifest's tools answer
one query at a time and hand results back — they do not own the calling
agent's conversation, so there is nothing in this layer to summarize. Checked
directly rather than assumed: `packages/octocode-awareness/src` has no
LLM-driven summarization step, but it is not silent on the topic either —
`endSession` accepts a caller-supplied `summary` field on the session record,
and `pi-hooks.ts` wires `handleSessionShutdown` to the *host's own*
`session_before_compact` and `session_shutdown` lifecycle events, so a
session is captured to persistent memory at the moment the host compacts or
ends it. That is reacting to host-level compaction, not performing it — a
different, complementary layer, not a gap disguised as a design choice.

Sub-agent isolation and "code mode" (Anthropic, StackOne) are, likewise, a
harness-level concern: a tool provider doesn't spawn or isolate sub-agents.
`octocode-awareness` — a separate package from the search/retrieval tools
this manifest documents — does carry adjacent primitives that don't appear
in any of the seven external sources reviewed for this section at all:
file-level locks (`fileLock`/`releaseFileLock`) and multi-agent handoff
(`registerAgent`/`agentSignal`) for *concurrent, cooperating* agents editing
the same repo, plus decay-scored cross-session memory retrieval
(`insertMemory`/`decayScore`/`findSimilarMemories`) and a verify-before-conclude
audit trail (`auditUnverified`/`markVerified`). Every article surveyed here
frames context engineering around a single agent's own loop; none addresses
concurrent multi-agent coordination. Worth stating precisely rather than
folding into this manifest's numbers: it is a different package solving a
different problem, not evidence for the retrieval claims above.

**The one gap that's real, not just unaddressed by design:** §13 already
states it as an open item and this section does not add a new claim on top —
no task-level benchmark (recall/precision/F1, wall-clock completion time)
exists for the routed loop against a baseline, the way Sourcegraph's
CodeScaleBench exists for a comparable 13-tool MCP server. That gap stays a
gap here; closing it is future work, not something this manifest asserts.

---

## 15. Conclusion

The claim this manifest makes is narrow on purpose: given a concrete handle
(an identifier, a shape, a path, an error string), route by what you already
hold (§3) through evidence graded by what it actually proves (§1), cross-check
across lanes before any claim of impact or absence (§5.5, §2b), and let the
tools' own hints, pagination, and schema tiering (§0, §9b, §10) keep the
reasoning budget on judgment instead of bookkeeping. Every mechanism behind
that claim was re-run against the live toolset while this document was
written, not asserted from memory of how the tools used to behave. That is
also this manifest's honest limit: it is a verified *procedure*, not a
measured *outcome* — §13's ratings are self-assessment, not a task-success
benchmark against a baseline, and §14 places it as the retrieval layer of a
larger context-engineering picture, not the whole of it. Where the strongest
lane can't answer, the next one down still can (§0's fallback ladder); where
this manifest itself
can't answer — task-level benchmarking, embeddings/KG lanes, >100k-file
scale — §1b and §13 name the gap rather than paper over it.

---

## Appendix: tool-agnostic mapping

The method (route → map → prove → cross-check → read) transfers to any toolset
with lexical/structural/semantic lanes. Octocode primitive → common equivalents:

| Octocode primitive | Generic equivalent |
|---|---|
| `localSearchCode` (text/regex, classified rows) | ripgrep (`rg -n --json`); classification (`declaration/callsite/comment`) you approximate manually |
| `localSearchCode mode:"structural"` + rule | ast-grep (`sg run -p / --rule`), tree-sitter queries, Comby |
| `lspGetSemantics` | Serena MCP, `mcp-language-server`, or any LSP client (definitions/references/callHierarchy) |
| `localGetFileContent minify:"symbols"` | aider repo-map (per-repo), ctags/tree-sitter outline (per-file) |
| `localGetFileContent matchString` + `matchRanges` | `rg -n -C<k>` then read the line spans; no merged-slice or anchor handoff — that's the gap |
| `localViewStructure` / `localFindFiles` | `tree`/`eza`, `fd` |
| `localBinaryInspect` | `file`/`objdump`/`nm`/`strings`, `tar -t`, `zipinfo` |
| `ghSearchCode` / `ghViewRepoStructure` / `ghGetFileContent` | `gh search code`, `gh api repos/.../git/trees`, `gh api .../contents` (same default-branch index limits apply) |
| `ghCloneRepo sparsePath` / `type:"directory"` | `git clone --depth 1 --filter=blob:none --sparse` + `git sparse-checkout set <path>` |
| `ghHistoryResearch` | `gh pr list/view`, `git log -- <path>`, `gh search prs` |
| `npmSearch` → `repositoryDirectory` | `npm view <pkg> repository`, then the repo's `directory` field |
| `oqlSearch` (federated) | no direct equivalent — compose the above manually |
| evidence grades + dual-lane cross-check (§1, §5.5) | pure method — apply with any of the above |

What does NOT transfer: prefilled `next.*` continuation queries, lineHint
self-correction, merged matchString slices, per-row match classification, and
typed empty-vs-error semantics — those are interaction-layer features of the
toolset itself, and the reason the loop is cheaper here than hand-composed.
