# Octocode Research Manifest

**Abstract.** Agentic code research fails in two symmetric ways. An LLM
reasoning without deterministic backing hallucinates line numbers and
generalizes from one grep; deterministic tools without an LLM directing them
cannot decide what the question even is. This manifest specifies the routing
algorithm that avoids both failure modes for Octocode's 13-tool surface (grep,
AST, LSP, and provider search spanning local disk, GitHub/npm, and a
federated query layer), and it describes how each tool actually behaves
rather than what its documentation claims. Where a behavior is config-gated
or a known gap, it is flagged inline instead of smoothed over. §0 through §2b
build the model, §3 through §8 are the operational algorithm, §9 through §11
are the mechanics, §12 and §13 are anti-patterns and known limitations, and
§14 places this work against the context-engineering literature.

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
generators (Factory AutoWiki, Devin DeepWiki, Google Code Wiki, LangChain
OpenWiki) solve a different problem, continuously-regenerated repo narrative,
and are consumed, not implemented, by this manifest: see §1c. For the retrieval
lanes this manifest deliberately does not use, see §1b. For where this manifest's
own retrieval-layer scope sits against the wider 2025–2026 context-engineering
literature (compaction, memory, sub-agent isolation), see §14.

**Contents.** [0](#0-the-thesis-agentic-research-not-agentic-guessing) The
thesis, [1](#1-the-core-model) Core model, [1b](#1b-the-lanes-this-manifest-does-not-use-embeddings-and-knowledge-graphs)
Lanes not used, [1c](#1c-existing-wikisdocs-as-a-lead-not-a-lane-not-proof)
Wikis as a lead, [1d](#1d-three-dimensions-read-together-structure-stream-connections)
Three dimensions, [2](#2-tool-matrix) Tool matrix, [2b](#2b-bulk-queries-parallelism-and-triangulation)
Bulk triangulation, [3](#3-the-router-master-decision-tree) The router,
[4](#4-where-does-the-code-live-local-vs-external-gate) Local/external gate,
[4b](#4b-matchstring-the-anchor-read-primitive-use-it-by-default) matchString,
[5](#5-local-algorithm-the-loop-in-full) LOCAL algorithm, [6](#6-external-algorithm-github--npm)
EXTERNAL algorithm, [7](#7-node_modules-first-before-any-external-hop) node_modules-first,
[8](#8-local--external-bridges) Local/external bridges, [9](#9-minification-modes-and-tradeoffs)
Minification, [9b](#9b-smart-schema-pay-for-the-contract-only-when-youre-about-to-use-it)
Smart schema, [10](#10-pagination-cursor-families) Pagination,
[11](#11-failure-semantics--recovery) Failure semantics, [12](#12-anti-patterns-each-observed-to-cost-real-round-trips)
Anti-patterns, [13](#13-strengths-and-known-limitations)
Strengths and limitations, [14](#14-where-this-sits-in-the-context-engineering-literature)
Literature position, [Appendix](#appendix-tool-agnostic-mapping) Tool-agnostic mapping.

---

## 0. The thesis: agentic research, not agentic guessing

Agentic research works when an LLM's judgment (what to look for, when evidence
is enough, which lane to trust, what "impact" or "unused" even means here) is
paired with tooling that is deterministic and semantic exactly where an LLM is
not: exact line numbers, exhaustive matches, provable call graphs, canonical
continuations. Octocode's tools exist to carry that second half so the agent's
reasoning budget goes to judgment, not bookkeeping. Reasoning without
deterministic backing hallucinates line numbers and "confirms" from one grep;
deterministic tools without an agent directing them can't decide what the
question even is. The local/external/federated loop in §1 isn't incidental
plumbing to route around. It's the mechanism that makes agentic research
work at all.

**The semantic fallback ladder.** When the strongest lane can't answer, the
next lane down still can, which is what makes "I don't know" rare instead of
a dead end: LSP unsupported for a language (§5, Rust `documentSymbols`) falls
back to `minify:"symbols"` (tree-sitter, language-wide, not LSP-dependent) or
lexical grep; GitHub's provider index returns `providerUnindexed` or a false
zero (§6) falls back to materializing the path and grepping it locally;
LSP call-hierarchy returns zero callers, which is a capability signal, not an
absence proof (§11), so cross-check lexically before concluding. Worked
example with the exact tool output: §5.5.

Seven mechanisms make the loop cheap in practice:

1. **Hints: the tool plans your next call.** Every `search` code row ships a
   prefilled, copy-paste OQL query for the obvious follow-up, not just a path.
   A text hit for a symbol returns `next.semantic` (a documentSymbols query
   against the containing file) and `next.fetch` (a content query anchored on
   the match), both directly executable. Deeper results add `nextHints[].why`
   and `confidence`, explaining *why* a continuation is offered, not just
   that one exists.
2. **Pagination: cursors are opaque, nothing silently drops.**
   `pagination.nextCharOffset` / `nextPage` are copy-only fields the agent
   never computes (§10); every paginator here is lossless by construction.
3. **Smart schema: pay for what you ask for, not what you might ask for.**
   Schema is tiered and opt-in. The tool catalog (`tools --json`, names and
   one-liners) is what an agent sees by default; a tool's full field-level
   schema (`tools <name> --scheme --json`) is far larger and is fetched only
   right before that tool is called raw. Same split for OQL: the full
   contract (`search --scheme`) is dense and complete, and the lean
   agent-facing guide (`search --scheme --compact`) is a fraction of the
   size and is what should be read first. Full breakdown: §9b.
4. **Smart moves across local/external/federated.** The bridge is explicit
   at three depths (§8: `ghCloneRepo`, `ghCloneRepo sparsePath`,
   `ghGetFileContent type:"directory"`); one call converts remote code to
   local-grade evidence. Treat those explicit bridge calls as the reliable
   mechanism; a plain `search` on a GitHub path does not reliably trigger
   materialization on its own, so don't assume it will. Separately: a query
   against `colinhacks/zod/packages` surfaces §7's warning directly. The
   current default branch is a `packages/` monorepo (v3/v4 split), not the
   flat `src/` layout an agent might assume. GitHub's tree is not the
   installed version's shape.
5. **Smart token management: minify on demand, never by default surprise.**
   Three views of the same file, in decreasing size: exact content, a
   `standard` view that strips comments and blanks, and a `symbols` view
   that keeps only signatures and constants with a line-gutter number. Pull
   the outline first (§9); pay for full bodies only on the lines that matter.
6. **Reasoning fields: state the goal before you fire, honestly scoped.**
   Every query object carries `id`, `mainResearchGoal`, `researchGoal`,
   `reasoning`. Traced through the source (`utils/response/bulk.ts`): `id`
   is resolved by `resolveQueryId()` and genuinely echoed back on every bulk
   result for correlation, but `mainResearchGoal`/`researchGoal`/`reasoning`
   are accepted and typed, then explicitly excluded from the echoed payload
   by name (`bulk.ts`'s `excludedKeys` set): accepted and typed, not (yet)
   consumed downstream. Their real value is forcing the *agent* to state
   what it's trying to learn and why this specific call advances it before
   the call fires. That's a discipline against impulsive, ungrounded queries,
   not a server-side feature. Use the discipline; don't oversell what isn't
   wired up yet.
7. **Multi-angle batching: one call, several angles, not just several
   independent lookups.** The 5-query batch (§2) isn't only for unrelated
   parallel lookups. Fire the SAME question through different lanes in one
   call and let disagreement between the angles be the finding. Worked
   example: §2b.

---

## 1. The Core Model

Three research surfaces, one loop:

```
LOCAL      workspace files, node_modules, cloned/materialized repos
           → localSearchCode, localGetFileContent, localViewStructure,
             localFindFiles, lspGetSemantics

EXTERNAL   GitHub (code, trees, files, PRs, commits) and npm
           → ghSearchCode, ghGetFileContent, ghViewRepoStructure,
             ghSearchRepos, ghHistoryResearch, npmSearch

FEDERATED  one typed query planned across both
           → oqlSearch (run `search --scheme` for the full contract)
```

Evidence has grades. Treat them differently:

| Grade | Source | Trust |
|---|---|---|
| **semantic** | LSP (definitions, references, callers) | Proven identity, but scoped to the language project; blind to scripts, re-exports-as-text, strings, docs |
| **structural** | AST match with metavar ranges | Proven shape: complete-node semantics, exact captures |
| **lexical** | ripgrep text/regex (rows come pre-classified: `kind: declaration/callsite/import/comment` + scoreHint) | Total coverage: sees everything, proves nothing about identity |
| **provider** | GitHub search index | Weakest: default-branch only, unindexed/archived repos return false zeros. `providerSemanticsApproximate` gives no line numbers |

**A core guideline:** avoid concluding from a single
grade. Semantic and lexical lanes each miss things the other catches. LSP
`callers` of a function can miss a diverged duplicate implementation in a
`.mjs` script or a barrel re-export; a grep pass catches both. Grep alone,
conversely, cannot distinguish a call from a comment. The *disagreement
between lanes is itself a finding*.

---

## 1b. The lanes this manifest does NOT use: embeddings and knowledge graphs

Two retrieval families exist in the field that this toolset deliberately omits;
know when they would beat you, and say so rather than pretending they don't exist.

**Indexed/semantic retrieval (embeddings, vector search).** Tools like
claude-context (embeddings+BM25) and grepai index the repo and answer *fuzzy
concept queries* ("where is retry logic handled?") without exact terms. If it
existed here it would slot into §1's table as **indexed**: high recall on
fuzzy concepts across huge unfamiliar corpora, proves nothing about identity,
adds index-staleness risk (results reflect the index, not the working tree).
Vendor token-reduction claims (40-97%) are mostly self-reported, so weigh
accordingly.

**When the deterministic loop still wins:** you hold *any* concrete handle
(identifier, filename, error string, code shape), and the router (§3) reaches
proof-grade evidence with zero index setup and zero staleness. **When an
index wins:** purely conceptual queries over very large unfamiliar codebases
where synonym-regex grep fans out too wide. This manifest's fallback for that
case is orientation (§3 "nothing" branch: tree, hotspot map, symbols
skeletons), slower than a good index, but always fresh and evidence-graded.

**Knowledge graphs / code graphs.** Graph tools precompute blast-radius and
impact edges. Here that job is done at query time by LSP call hierarchy plus
the mandatory cross-check (§5.3-5.5): correct per-query, but not precomputed.
For repo-wide impact sweeps at very large monorepo scale, expect a
precomputed graph to be materially cheaper; this loop has not been validated
at that scale.

**Positioning in one line:** deterministic-first, evidence-graded, zero-index,
and honest that fuzzy-concept recall at mega-scale is the one job where an
indexed lane is the better entry point (the router still applies for the
verification steps that follow).

---

## 1c. Existing wikis/docs as a lead: not a lane, not proof

AutoWiki-style tools (Factory AutoWiki, Devin DeepWiki, Google Code Wiki,
LangChain OpenWiki) generate and continuously refresh repo-level narrative
docs, architecture summaries, module maps, sometimes a chat layer, synced
via `git diff` on push. When a repo already has one (`ARCHITECTURE.md`,
`droid-wiki/`, `openwiki/`, `.devin/wiki.json`, a GitHub Wiki tab, or a
DeepWiki/Code Wiki page), it is a **fast orientation lead**, not a new evidence
grade: treat its claims exactly like a provider snippet, useful for naming
entry points and shaping the first query, worthless for "impact is X" or
"unused" without the §5.5 cross-check. Two failure modes to watch for: the
doc describes an old shape of the code (staleness, worse than no doc, because
it's confidently wrong), and the doc's own confidence reads as proof when it
isn't. This manifest does not generate or maintain such docs (out of scope,
that is the cited tools' job); it only consumes them as a router input (§3).

---

## 1d. Three dimensions, read together: structure, stream, connections

Every codebase question can be approached from three angles, each individually
incomplete and only additive when combined:

```
STRUCTURE    the tree: where things live, how big, how named, how nested
             → localViewStructure, ghViewRepoStructure, localFindFiles

STREAM       the text: what is actually written, exact bytes or an outline
             → localGetFileContent, ghGetFileContent, localSearchCode (lexical)

CONNECTIONS  the graph: who calls/imports/extends/implements what, proven shapes
             → lspGetSemantics, localSearchCode mode:"structural"
```

None of the three substitutes for the others, and each is silent about what
it can't see rather than flagging the gap:

- **Structure alone** gives layout, not meaning: a file named `auth.ts` sitting
  next to `session.ts` proves nothing about what either does, or whether they
  actually interact.
- **Stream alone** (grep/read) sees everything but proves nothing about
  identity: it cannot tell a real call from a comment, a string, or a
  same-named symbol in an unrelated scope (§1's lexical grade).
- **Connections alone** (LSP/AST) is proof-grade but capability-scoped:
  unsupported languages, dynamic dispatch, string-based wiring, and
  non-project scripts are invisible to it (§1's semantic grade; §11's
  `serverUnavailable` means capability absence, not "no usage").

This is not a fourth lane on top of §1's evidence grades, it is a naming of a
pattern that is already load-bearing elsewhere in this manifest, made
explicit so it is reached for on purpose rather than by accident: §5's
ORIENT → MAP → PROVE sequence walks exactly structure → stream → connections
for one target; §3's "nothing" branch opens with a tree call (structure)
before a hotspot grep (stream); §2b's triangulation worked example is a
stream/connections disagreement on the same claim, and the mismatch itself
was the finding. Combined orientation is where those threads generalize:
before concluding anything nontrivial about an unfamiliar area, pull at
least two of the three dimensions, not just the one that happens to be
cheapest or most habitual. An agent that only ever greps (never checks
structure, never proves with LSP) or only ever calls LSP (never rereads the
tree, never falls back to text) builds a systematically partial model of the
code and has no signal that it's partial, because each dimension only
answers what it was asked and stays silent on the rest. The cost of the
extra dimension is small (one `localViewStructure`, one
`minify:"symbols"` pass, §9) relative to the cost of a wrong "impact is X"
or "this is unused" built on a single angle (§12, item 2).

---

## 2. Tool Matrix

| Tool | Surface | Role | Reach for it when |
|---|---|---|---|
| `localSearchCode` | local | text/regex/AST search, count modes, ranked | any local content question, the workhorse |
| `localGetFileContent` | local | read file / matchString slices / line ranges | reading after you have coordinates |
| `localViewStructure` | local | directory tree | orientation in an unfamiliar dir |
| `localFindFiles` | local | find by name/size/time/permissions metadata | the constraint is *about the file*, not in it |
| `lspGetSemantics` | local | definitions, references, callers/callees, hover, symbols, types | proving identity and impact |
| `ghSearchCode` | external | GitHub code/path search | locating code across repos you don't have |
| `ghGetFileContent` | external | read GitHub file (slices/ranges/symbols); `type:"directory"` materializes a subtree | reading remote files; bridging remote→local |
| `ghViewRepoStructure` | external | GitHub tree browse | orienting in a remote repo |
| `ghSearchRepos` | external | repo discovery | finding candidate repos/prior art |
| `ghHistoryResearch` | external | PR search + PR deep-read + commit history | archaeology: why did this change |
| `npmSearch` | external | package → source repo (+ `repositoryDirectory`) | resolving a dependency to its home |
| `oqlSearch` | both | typed federated query; research/graph/diff targets | multi-predicate queries, remote+local in one plan |
| `ghCloneRepo` | bridge | full/sparse clone (**gated: `ENABLE_CLONE=true`**) | whole-repo local analysis |

Bulk: every tool takes up to 5 parallel queries per call with per-query `id`.
Batch independent probes into ONE call: it is the cheapest parallelism you have.
That is only half the value of the batch. See §2b for using it to triangulate
one question instead of just parallelizing unrelated ones.

---

## 2b. Bulk queries: parallelism AND triangulation

The 5-query batch is usually read as a speed feature (5 independent lookups,
1 round-trip). It is also a **correctness** feature: fire the same question
through different lanes (lexical, structural with one shape, structural with
another shape) in a single call, and treat any disagreement between the
angles as the actual finding, not noise to average away.

Worked example: one `localSearchCode` bulk call against this repo's own
`packages/octocode-awareness/src/db.ts`, asking one question three ways:
*"is the `_db` module singleton ever reassigned or read-guarded outside
`connectDb`?"*

| Angle | Query | Result |
|---|---|---|
| lexical | `keywords:"_db\s*="` (regex) | 1 hit, the assignment inside `connectDb` |
| structural (assignment shape) | `pattern:"_db = $VAL"` | 1 hit, AST-proven, same assignment, now identity-confirmed |
| structural (guard shape, guessed) | `pattern:"if (!_db) { $$$BODY }"` | **0 hits** |

The third angle's zero looked like "no read-guard exists." It was wrong about
the *shape*, not the *fact*. A follow-up lexical angle (`matchString:"!_db"`)
immediately found `if (!_db) throw new Error('Database not connected...')` in
`getDb()`, a real guard, just brace-less, so the guessed AST pattern
(`{ $$$BODY }`) could not match a single-statement `if`. One angle alone would
have closed the question wrong ("no guard"); three-plus angles in one batch
turned the mismatch itself into a finding (§11: structural 0 matches is a
shape signal, not an absence proof) instead of a silently wrong conclusion.

**When to spend a batch slot on a second angle instead of a second file:**
whenever the question is a claim ("this is unused", "this is never
reassigned", "this is always guarded") rather than a location lookup. Location
lookups want 5 independent files/paths; claims want 2-3 angles on the *same*
target: lexical for total coverage, structural for shape-proof, and (per
§5.5) semantic when a callable identity is in question.

---

## 3. The router (master decision tree)

Route by **what you already hold**, not by a fixed pipeline order.
Running grep first when you hold a symbol name wastes a hop; running LSP first
when you hold only a concept cannot work at all. Whichever branch you enter,
the destination is still at least two of §1d's three dimensions
(structure/stream/connections) before you conclude anything, not just the
one the branch started with.

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

`localSearchCode` over `node_modules/zod` (with `excludeDir: []` and
`noIgnore: true`, since node_modules is excluded by default) finds the exact
installed source fast, with the version that actually runs, which GitHub's
default branch is NOT guaranteed to be.

---

## 4b. matchString: the anchor-read primitive (use it by default)

`matchString` on `localGetFileContent` / `ghGetFileContent` is the highest-leverage
read mode in the toolset. Instead of guessing line ranges or paging a whole file,
you hand it the string (or regex) you care about and get back only the relevant
slices, **plus machine-usable anchors for the next step**. This works the same
way on workspace files and on clone-materialized files:

```
localGetFileContent / ghGetFileContent
  matchString: "sanitizeStructuredContent"       ← literal, case-insensitive default
  matchString: "export function (decayScore|decayComponents)"
    + matchStringIsRegex: true                    ← regex anchors
  contextLines: N                                 ← raise to capture whole bodies
```

What you get:
- **Merged slices, not N reads**: repeated occurrences of the same string in
  a file come back merged into a handful of slices with real
  `... [N lines omitted] ...` separators, one call instead of one read per
  occurrence.
- **`matchRanges[]`**: exact `{start,end}` line ranges per slice. Feed them
  straight into `startLine/endLine` follow-ups or LSP `lineHint`.
- **The warning text names your anchors**: a message like "Found occurrences
  on lines X, Y, Z, and more. These lines are lineHint anchors for
  lspGetSemantics." The tool is literally handing you the next call.
- **Regex mode fetches multiple related definitions in one read** (several
  function definitions plus their signatures via one regex, with surrounding
  context lines).
- **Works identically remote**: same anchors from a GitHub file, so you can go
  ghSearchCode (which file) to ghGetFileContent matchString (which lines) to
  materialize to LSP at those lines, without ever reading a full file.
- **The federated `search` shorthand goes further than a warning string**: a
  code-search row's `next` object is a directly-executable OQL query object,
  not prose. `next.fetch` and `next.semantic` come back pre-populated with
  `from`, `target`, and `params` filled in (§0). Some results add
  `nextHints[].why` and `confidence` explaining *why* that continuation was
  offered, e.g. `"Read the code at this symbol location."` /
  `confidence:"exact"`. That's reasoning support, not just a pointer.

Default read policy: **matchString first, line ranges second, fullContent last**
(small files only). If you know *what* you're looking for but not *where*, this
is always the cheapest correct read. Related but different: `ghHistoryResearch
matchString` filters PR patches/comments to matching sections (same idea,
different surface).

---

## 5. LOCAL algorithm (the loop, in full)

Steps 0-3 below are §1d's three dimensions in sequence for one target:
ORIENT is structure, MAP is stream (outlined), PROVE is connections. Step 5
folds stream back in as a cross-check on what connections alone proved.

```
0. ORIENT     (skip if you know the area)
   localViewStructure         - shape of the directory
   countMatchesPerFile        - which files carry the concern, instantly: one
                                `keywords:"session"` call over
                                octocode-awareness/src ranked sessions.ts,
                                pi-hooks.ts, intents.ts, db.ts at the top,
                                a hotspot map with zero snippets read

1. LOCATE     (router entry, §3)

2. MAP        localGetFileContent minify:"symbols" on the winning file: a
              small skeleton (§9), every signature kept with a line-gutter
              number that is a ready LSP anchor. This one call often
              answers concept questions outright.

3. PROVE      lspGetSemantics from a real anchor (grep line / symbols line):
              - callers/callees/callHierarchy for callables (impact analysis)
              - references includeDeclaration:false, groupByFile:true for the rest
              - lineHint SELF-CORRECTS (passed 261, resolved to 263 and
                reported `foundAtLine`; reproduced on a different symbol:
                passed 260, resolved to 264), but avoid guessing it from nothing
              - READ the completeness block: truncatedByDepth, dynamicCallsExcluded,
                stdlibCallsExcluded, failedRequestCount tell you what you did NOT see

4. EXTRACT    localSearchCode mode:"structural" when you need the complete node or
              typed captures. On this repo's own code
              (`packages/octocode-awareness/src/db.ts`), a pattern like
              `db.exec($$$SQL)` captures a multiline `CREATE TABLE` statement
              whole, as one `$SQL` metavar, something grep only sees one line
              of inside a many-line template literal. Isolating a single call
              out of several matches needs the pattern **nested inside** a
              rule, not passed alongside it:
              `rule: "pattern: db.exec($$$SQL)\nhas:\n  regex: ALTER TABLE\n  stopBy: end"`.
              Passing `pattern` and `rule` as sibling fields is rejected
              (`"provide either pattern or rule, not both"`).
              Rules of the mode:
              - a `pattern` must match a COMPLETE AST node (body, return type, all
                required syntax). Partial shape gives 0 matches (pattern queries get
                a guidance warning; rule queries currently do not).
              - method calls are not plain calls: on the same file, bare
                `exec($$$A)` matches nothing while `$RECV.exec($$$A)` matches
                the identical calls. `foo($$$A)` will not match `x.foo($$$A)`.
              - for partial/relational matches use a YAML rule (kind/has/inside/not/any,
                stopBy: end) with `pattern` nested inside the rule string, not as a
                sibling field. Bare rule YAML and `rule:`-wrapped are both accepted
                on recent engine versions; older engines require the `rule:` wrapper.
              - `$$$LIST` captures currently include comma separators as elements, so filter them.

5. CROSS-CHECK  (non-negotiable before "impact is X" / "unused" / "only used in Y")
              One package-wide grep of the symbol INCLUDING tests/scripts/configs.
              Diff lexical hits vs semantic hits. Every lexical hit LSP didn't report
              is a finding: re-export, shadow copy, string/SQL/config reference, doc.
              (This exact step can expose a diverged duplicate scorer in a
              skills/*.mjs script that LSP callers cannot see. Separately,
              `callHierarchy` can report zero incoming calls for a symbol
              registered through a dispatch table
              (`dynamicCallsExcluded`), while a plain lexical count in the
              same call finds real occurrences the semantic lane missed.)

6. READ       matchString first (§4b): merged slices plus matchRanges anchors;
              startLine/endLine when you already hold exact coordinates.
              minify:"none" whenever you will quote, diff, or edit. Lossy modes
              rewrite whitespace and quote style (standard mode rewrites `'..'`
              to `` `..` ``).
```

LSP capability notes: pull `diagnostic` may be unsupported (the server
pushes instead, and the tool says so rather than returning a fake empty); native
`documentSymbols` is JS/TS-only, so for Rust and others use `localGetFileContent
minify:"symbols"`, which is tree-sitter based and language-wide.
`serverUnavailable`/`unsupported` means capability absence, NOT "no usage".

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
              - minify:"symbols" works on GitHub files too, at essentially the
                same ratio as local for the same file content (§9). A GitHub
                default branch a few commits behind the local working tree
                will differ slightly, but the compression behavior is the
                same. Minification is not a local-only shortcut, so orient
                before pulling bodies
              - startLine/endLine for known ranges; fullContent only for small files

4. WHY        ghHistoryResearch:
              - PR triage: keywords + match:["title"] + concise:true, then prNumber +
                content selectors (body/patches mode:"selected"/comments) for depth
              - archaeology: state:"merged" sort:"created" order:"asc"
              - commit lane: type:"commits" owner/repo/path (trailing "/" = subtree)

5. ESCALATE   the moment you need AST, LSP, multi-file grep, or >2 more reads:
              materialize and go local (§8).
```

**GitHub index blind spots (known):** default-branch-only; archived
repos return zero code hits; renamed repos redirect for content APIs but silently
fail for search; the code-search API has an announced upstream deprecation.
Therefore: **an empty ghSearchCode is NOT absence.** Verify with
`ghViewRepoStructure` (does the path exist?) or `ghGetFileContent`, or materialize
and grep locally. Avoid reporting "X does not exist in repo Y" from provider search alone.

---

## 7. node_modules FIRST (before any external hop)

The installed dependency is (a) already on disk, (b) the exact version that runs,
(c) searchable in milliseconds. GitHub shows you a default branch that may be
newer, older, or restructured.

```
Question about a dependency's behavior?
  1. localViewStructure node_modules/<pkg>          - what shipped (dist? src? types?)
  2. localSearchCode path:node_modules/<pkg>        - MUST set excludeDir: []
       + noIgnore: true                               (defaults skip node_modules)
  3. localGetFileContent on the hit                 - .d.ts and shipped src are gold
  4. LSP hover/definition often resolves INTO node_modules types for free
       when anchored from your own importing file
  5. ONLY IF the answer isn't in the shipped artifact (needs git history, tests,
     unshipped sources): npmSearch, then the repo, then the §6 external loop
```

Gotchas: `excludeDir: []` is mandatory. The default exclusion list
silently skips node_modules, and a "no matches" there means "didn't look."
Watch for dual hits (src/ + dist/ in the same package); prefer the one your
resolver actually loads when semantics matter.

**A scoping nuance worth knowing exactly:**
pointing `path` directly AT `node_modules/<pkg>` reaches it fine with no
`excludeDir` override at all. The default exclusion filters directory names
encountered *while walking*, not the root path you hand it. But searching a
parent path (e.g. `.`) with an explicit `include: "node_modules/**"` glob is
still fully blocked by default (`zeroMatches`). `include` does not override
`excludeDir`; only clearing `excludeDir` (`""`/`[]`) plus `noIgnore:true`
does, and once cleared the same query reaches files nested several
`node_modules` levels deep (e.g. a transitive dependency's own bundled
`node_modules`).
So scoping `path` straight into a known package is always safe. Scoping a
wider search and hoping `include` reaches into `node_modules` is not.

A path-scoped search into a dependency's current default branch on GitHub
can show a very different layout than what's installed (a monorepo split,
a rename, a restructure) from what an agent might remember or assume from
an older snapshot. The installed copy under `node_modules` is unaffected by
any of that upstream restructuring; it is still the one true source for
"what actually runs."

---

## 8. LOCAL ↔ EXTERNAL bridges

### External to Local (materialize, then analyze): three depths

All gated by `ENABLE_CLONE=true` (off → typed error saying exactly that).
The full §5 loop runs unmodified on the result at any of the three depths:

| Depth | Call | What lands on disk | Use when |
|---|---|---|---|
| **file** | `ghCloneRepo` + `sparsePath: "path/to/file.ts"` | sparse checkout: the file's subtree **plus repo-root files** (README, package.json, configs; git sparse-checkout keeps root); `complete:false` flagged | one file needs repeated matchString/LSP reads |
| **tree** | `ghGetFileContent type:"directory"` | just that subtree under `~/.octocode/tmp/tree/<owner>/<repo>/<branch>/...`, with `commitSha`, per-reason skip accounting (`oversized`/`binary`/`fileLimit`/...) and disclosed size/count limits; partiality warning when limits bite | analyzing one directory |
| **repo** | `ghCloneRepo` (no sparsePath) | full shallow clone, `complete:true`, cached for a period (`forceRefresh` to bust) | repo-wide grep/AST/LSP, dead-code, reachability |

Every result carries `localPath` + prefilled `next.localSearch` / `next.viewStructure`.
OQL reaches the same machinery via `materialize: "auto"/"required"` or a row's
`next.materialize`.

**Post-bridge:** structural AST rules, native LSP `documentSymbols`, and
matchString anchor reads all run unmodified on the materialized paths. Remote code
becomes fully local-grade evidence after ONE bridge call.

**When to materialize:** you need AST/structural, LSP, multi-file regex, or you're
about to make a 3rd+ read call into the same remote area.

**Honesty caveats (from the tool itself, respect them):** tree materialization is
bounded, so check `skipped` counts before any "not present" claim on a materialized
tree; sparse clones are `complete:false` by definition; prefer depth=repo before
repo-wide reachability/dead-code conclusions (the warning says exactly this).

### Local → External (context enrichment)

- symbol came from a dependency → §7 first, then npmSearch → repo → docs/tests/history
- "why is this code like this" → `ghHistoryResearch type:"commits"` on the file path,
  then the PR behind the commit (`reviewMode:"full"` for the whole story)
- "has someone solved this" → `ghSearchRepos` (concise triage) → §6 on candidates

### Federated in one shot (OQL)

`oqlSearch from:{kind:"github",owner,repo}` plans provider search + optional
materialization for you. Behaviors to rely on:
- GitHub code rows come back `proofGrade:"text"` with `evidence.answerReady:false`
  and a `providerSemanticsApproximate` diagnostic. That is NORMAL, not failure;
  each row carries a prefilled `next.fetch` to upgrade to exact content
- `target:"research"` page 1 is summary counts, packets from page 2 onward;
  `next.graph` upgrades rows to proofStatus (confirmed-by-lsp / conflicting-evidence / etc.)
- zero rows plus `providerUnindexed` does not mean absence; follow `next.materialize`
- run `search --scheme` before authoring nontrivial OQL; `--explain` shows the routing PLAN and then also executes, while `--dry-run` prints the PLAN without executing

---

## 9. Minification: modes and tradeoffs

| Mode | What it does | Relative size | Use when | Avoid when |
|---|---|---|---|---|
| `none` | verbatim bytes | full size | quoting, diffing, editing, regex-sensitive reads | n/a |
| `standard` (default) | strips comments/blanks, compacts whitespace, may rewrite quote style | noticeably smaller, file-dependent on comment and blank-line density | general reading | anything you'll quote verbatim, since it is LOSSY (rewrites `'x'` to `` `x` ``, drops comments) |
| `symbols` | signatures + constants outline with `NNN\|` line gutter | smallest by far, file-dependent | orientation, building an anchor sheet, API surface review | reading logic bodies |

Works identically on local and GitHub files. The `symbols` gutter numbers are
valid `lineHint`/`startLine` anchors.

Search-side equivalents: `concise:true` (gh tools) for triage lists,
`filesOnly`/`countMatchesPerFile` (local) for maps, `format:"compact"`/`groupByFile`
(LSP) for wide result sets, `content.patches mode:"selected"` + `ranges` (PRs) to
avoid whole-diff pulls.

---

## 9b. Smart schema: pay for the contract only when you're about to use it

Schemas are large by nature (every field, type, bound, default), and an agent
does not need most of them most of the time. The design answer is tiering,
not omission:

| What you ask for | Relative size | When |
|---|---|---|
| `tools --json`: tool catalog, names + one-liners | small | default orientation: which of the 12 tools is this? |
| `tools <name> --scheme --json`: one tool's full field-level schema | large | right before calling that tool raw, avoid guessing a field |
| `search --scheme --compact`: lean OQL agent guide (source/target/recipes) | small | read this first for any non-trivial OQL query |
| `search --scheme`: full OQL contract (every target/predicate/param) | large | only when the compact guide didn't resolve an edge case |

The compact OQL guide is a small fraction of the size of the full contract
and answers the routing question ("which target, which source") that blocks
most queries. Read it before the full contract, not instead of it when
something is still ambiguous. The same logic applies one level up: read the
tool catalog before any single tool's schema, and read a single tool's
schema before ever calling it with guessed field names (§12 anti-pattern:
avoid guessing a field that a one-line schema read would have shown you).

---

## 10. Pagination: cursor families

**Key principle: cursors are OPAQUE. Copy them from the response
(`pagination.nextCharOffset`, `nextPage`, `next.*` prefilled queries) and avoid
computing your own.** Every paginator here is lossless; nothing is silently dropped.

| Family | Fields | Tools | Behavior |
|---|---|---|---|
| Char window (file) | `charOffset`/`charLength` → `nextCharOffset`, `isPartial` | local/gh GetFileContent | a capped `charLength` on a large file returns `pagination.hasMore:true` plus a ready, copy-paste `next.charRange` query pre-filled with the next offset, and `nextHints.why:"Read the next content window."` explaining the offer. Nothing to compute, nothing silently dropped. |
| Result page | `page` → `hasMore`/`nextPage` | all search tools, structure tools | later pages return the next slice of rows with a `reported`/`reachable`/`capped` breakdown |
| Per-file match page | `matchPage` + `maxMatchesPerFile` | localSearchCode | walks a noisy file without re-fetching others |
| List pages | `itemsPerPage` + `page`; `filePage`/`commentPage`/`commitPage` | LSP lists, PR content surfaces | large symbol lists page cleanly across calls |
| Response window | `responseCharLength`/`responseCharOffset` | EVERY tool (outermost) | wraps the whole bulk response; advance only on `hasMore`. Wrinkle: on multi-query bulk repo-search, prefer bigger `responseCharLength` or per-query pages over advancing this offset |

Budget levers, cheapest first: tighter scope (path/owner/repo/langType) → leaner mode
(concise/symbols/filesOnly/counts) → smaller pages (`maxFiles`, `maxMatchesPerFile`,
`limit`) → THEN paginate what's left. GitHub `matchIndices` are snippet offsets, not
line numbers, so get lines from `ghGetFileContent matchString`.

---

## 11. Failure semantics & recovery

| Signal | Meaning | Move |
|---|---|---|
| `status:"empty"` + stats (`filesSearched`, `bytesSearched`) | proven negative *for that scope* | widen scope / synonyms / drop filters; only then conclude, and quote the stats |
| `status:"error"` + `errorCode` + hint | typed failure with a `repair` field naming the recovery move (e.g. a nonexistent local path returns *"Verify the path exists (orient with target:'structure' on a known-good parent), fix typos, or materialize the remote source first."*) | follow the hint; do not retry verbatim |
| structural 0 matches | usually an incomplete pattern, NOT absence | add `$$$BODY`/return type, or switch to a rule; check the guidance warning (example, §2b: `if (!_db) { $$$BODY }` → 0 hits on a real, brace-less `if (!_db) throw ...` guard) |
| LSP `serverUnavailable`/`unsupported` | capability absence, NOT "no usage" | fall back to grep / symbols view |
| LSP `completeness.complete:false` | results truncated by depth/dynamic-call exclusion | deepen or supplement with grep before claiming full impact |
| gh empty / `providerUnindexed` | index blind spot, NOT absence | verify path exists → materialize → grep locally |
| `resolvedBranch` ≠ requested | ref fell back to default branch | re-check which branch you're actually reading |
| `warnings[]` | redaction, fallback engine, pre-filter skips, pagination notes | read them, they change what the result means |

`Pre-filter skipped parsing N file(s) (literal anchor absent)` on structural results
is an optimization disclosure, not data loss: those files could not contain the
literal anchor.

---

## 12. Anti-patterns (each observed to cost real round-trips)

1. **Fixed pipeline instead of routing.** grep→AST→LSP is not a law; enter where
   your knowledge already points (§3).
2. **Concluding impact from one evidence lane.** The lexical/semantic diff is the
   deliverable, not a nicety (§5.5).
3. **Trusting GitHub search zeros.** Default-branch-only + unindexed repos =
   false absence machine (§6).
4. **Reading before mapping.** `minify:"symbols"` first; bodies only for the slices
   that matter (§5.2, §9).
5. **Skipping node_modules.** You debug the installed version, not the default branch (§7).
6. **Guessing lineHint / computing charOffset.** Anchors come from prior results;
   cursors come from `pagination.*` (§5.3, §10).
7. **Staying remote too long.** 3+ reads into one remote area ⇒ materialize (§8).
8. **Ignoring `next.*` hints.** They are prefilled, correct continuation queries.
   The tool has already planned your next call, including a self-correcting
   grep-to-LSP anchor handoff.
9. **quoting from `minify:"standard"` output.** It rewrites quotes and strips
   comments; quote only from `minify:"none"` (§9).
10. **Serial single queries.** Up to 5 queries per call, per tool. Batch.
11. **Calling a raw tool with a guessed field name instead of reading its schema
    first.** The catalog and the OQL compact guide are both cheap to read;
    a wrong field costs a full extra round-trip and, worse, a silently-ignored
    parameter (§9b).
12. **Spending a claim on one angle instead of one batch on several.** "Is X
    unused / always guarded / never reassigned" is a claim, not a location
    lookup. One structural pattern guessing the wrong shape returns a clean
    0 that reads exactly like a proven negative (§2b: a real brace-less
    guard, invisible to one guessed pattern, obvious to a second lexical
    angle in the same batch).

---

## 13. Strengths and known limitations

Read this as a summary of the evidence already shown, not a new claim, and
not a scorecard. Each algorithm's strengths and the gaps that limit it:

| Algorithm | Strengths | What holds it back |
|---|---|---|
| **LOCAL loop (§5)** | Full evidence stack (lexical + structural + semantic + binary) with self-correcting anchors, merged matchString slices, honest completeness metadata, and typed recovery on every failure. | The lexical/semantic cross-check is agent discipline rather than a tool-emitted delta, and per-language LSP gaps remain (Rust documentSymbols, pull diagnostics). |
| **EXTERNAL/GitHub loop (§6)** | Orient, search, matchString-read, history is a strong sequence, and symbols/matchString work identically remote. | Capped by provider physics the tool can't fix: default-branch-only index, archived/renamed blind spots, no remote AST/LSP. The clone bridge is the mitigation; escalating early effectively lifts this to local-grade. |
| **NPM / node_modules-first (§7)** | The installed-version-is-ground-truth rule is cheap and correct; npmSearch resolves package→repo→`repositoryDirectory` in one call. | The `excludeDir: []` footgun (a forgotten default silently skips node_modules) and src/dist dual-hit ambiguity. |
| **Bridge: external→local (§8)** | One call converts remote code to full local-grade evidence at any of three depths, with per-file skip accounting and self-describing partiality. | Config-gated (`ENABLE_CLONE`), and tree-depth limits require reading the `skipped` counts. |

**Standing gaps**, stated plainly rather than scored away. No embeddings/KG
lane (addressed as a position statement in §1b, not as tooling). Everything
above describes deterministic mechanisms, not task-success rates: **open: a
task-level A/B benchmark of this routed loop against a grep-only baseline
does not exist** (§14 returns to this point rather than restating it as
solved). Scale beyond a large monorepo is untested. And the lexical/semantic
cross-check (§5.5) is agent discipline enforced by this document, not a
delta the tools compute and emit themselves. What differentiates this
algorithm from a generic grep-AST-LSP pipeline is route-by-what-you-hold
(§3), node_modules-first (§7), typed failure semantics (§11), and
anchor-passing ergonomics (§4b), but "differentiated" is a design claim, not
a measured one. The gaps above are what would need to close before it
could become one.

---

## 14. Where this sits in the context-engineering literature

Since late 2025, "context engineering" (curating the finite token budget an
LLM sees per turn) has converged into a named discipline with a shared
vocabulary (Anthropic, [Sep 2025](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents);
Sourcegraph, [2026](https://sourcegraph.com/blog/context-engineering)). That
literature splits the problem into four layers: instructions, retrieval,
memory, and tools. This manifest is entirely about one of those four,
**retrieval**, and about the tool-provider side of it specifically, not the
agent-harness side. Naming that boundary precisely, instead of implying this
manifest covers ground it doesn't, is the point of this section.

**Where the manifest's mechanisms match the field's best-known patterns:**

| Field pattern | Source | This manifest's answer | Verdict |
|---|---|---|---|
| Just-in-time retrieval: hold a lightweight identifier, load content on demand instead of pre-loading it | Anthropic | `matchString` anchors, `next.fetch`/`next.semantic` (§4b). The toolset never had a pre-load path to begin with | Matches by construction |
| Memory pointers: a short reference token stands in for content that can be re-fetched | StackOne | Materialized `localPath` + `next.materialize` (§8); OQL rows carry the same pattern | Matches |
| Built-in filters as the pragmatic middle ground for tool *providers* (vs. sandboxed code-mode, which StackOne calls "heavy... most won't build it") | StackOne | `countMatchesPerFile`, `filesOnly`, `discovery`, structural metavars, `concise` (§9, §12) | Matches: independent validation of an existing design choice, not self-assessment |
| Tiered/on-demand schema so tool definitions don't burn the budget up front | StackOne ("Tool Definition Catch-22") | §9b: a small tool catalog vs. a much larger per-tool schema; a compact OQL guide vs. the full contract | Matches: same discipline applied at the field level since the tool count (13) never grew large enough to need discovery-by-search |
| Pre-flight cost awareness: let the agent see or estimate cost before committing | StackOne ("dry-run... their survival becomes their responsibility") | `search --dry-run` plans an OQL query without executing it (prints the routing PLAN and an evidence line, no result rows); `--explain` shows the same PLAN but then executes | Partial: the non-executing planner ships and is CLI-reachable. An `estimatedTokens` field and a 30k/50k token-warning path exist in `utils/pagination/{core,hints}.ts` but are currently unwired into any CLI-reachable response (dead code), so the cost-warning half is aspirational, not shipped |
| Structural/semantic code intelligence beats plain text retrieval for coding agents, measured | Sourcegraph (CodeScaleBench: file recall 0.127 to 0.277, P@5 0.140 to 0.478, F1@5 0.099 to 0.262 with an MCP code-graph layer vs. grep-only) | §1/§5 evidence grades (semantic > structural > lexical > provider) argue the same ordering qualitatively | Same conclusion, weaker proof: this manifest has never run the equivalent task-level A/B (§13 already lists this as an open item, not restated as new) |

**Where this manifest doesn't compete, on purpose:**

Compaction (Anthropic, the Claude context-engineering cookbook, and
LangChain's Deep Agents SDK all treat LLM-driven summarization of an agent's
own conversation history as one of 2-3 mandatory levers for long-horizon
tasks) has no equivalent here, and shouldn't. This manifest's tools answer
one query at a time and hand results back. They do not own the calling
agent's conversation, so there is nothing in this layer to summarize. Checked
directly rather than assumed: `packages/octocode-awareness/src` has no
LLM-driven summarization step, but it is not silent on the topic either.
`endSession` accepts a caller-supplied `summary` field on the session record,
and `pi-hooks.ts` wires `handleSessionShutdown` to the *host's own*
`session_before_compact` and `session_shutdown` lifecycle events, so a
session is captured to persistent memory at the moment the host compacts or
ends it. That is reacting to host-level compaction, not performing it: a
different, complementary layer, not a gap disguised as a design choice.

Sub-agent isolation and "code mode" (Anthropic, StackOne) are, likewise, a
harness-level concern: a tool provider doesn't spawn or isolate sub-agents.
`octocode-awareness`, a separate package from the search/retrieval tools
this manifest documents, does carry adjacent primitives that don't appear
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
states it as an open item and this section does not add a new claim on top.
No task-level benchmark (recall/precision/F1, wall-clock completion time)
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
reasoning budget on judgment instead of bookkeeping. That is also this
manifest's honest limit: it describes a *procedure*, not a measured
*outcome*. §13's assessment is a qualitative summary, not a task-success
benchmark against a baseline, and §14 places it as the retrieval layer of a
larger context-engineering picture, not the whole of it. Where the strongest
lane can't answer, the next one down still can (§0's fallback ladder); where
this manifest itself can't answer (task-level benchmarking, embeddings/KG
lanes, very large monorepo scale), §1b and §13 name the gap rather than
paper over it.

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
| `localGetFileContent matchString` + `matchRanges` | `rg -n -C<k>` then read the line spans; no merged-slice or anchor handoff, that's the gap |
| `localViewStructure` / `localFindFiles` | `tree`/`eza`, `fd` |
| `ghSearchCode` / `ghViewRepoStructure` / `ghGetFileContent` | `gh search code`, `gh api repos/.../git/trees`, `gh api .../contents` (same default-branch index limits apply) |
| `ghCloneRepo sparsePath` / `type:"directory"` | `git clone --depth 1 --filter=blob:none --sparse` + `git sparse-checkout set <path>` |
| `ghHistoryResearch` | `gh pr list/view`, `git log -- <path>`, `gh search prs` |
| `npmSearch` → `repositoryDirectory` | `npm view <pkg> repository`, then the repo's `directory` field |
| `oqlSearch` (federated) | no direct equivalent, compose the above manually |
| evidence grades + dual-lane cross-check (§1, §5.5) | pure method, apply with any of the above |

What does NOT transfer: prefilled `next.*` continuation queries, lineHint
self-correction, merged matchString slices, per-row match classification, and
typed empty-vs-error semantics. Those are interaction-layer features of the
toolset itself, and the reason the loop is cheaper here than hand-composed.
