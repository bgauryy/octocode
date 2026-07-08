# Research Algorithm

The thinking core of `octocode-research`: router, reading dimensions, evidence model, triangulation, and failure playbook.
Algorithm.md is the single source for these rules. Read it once per session before the mode-specific flows.

Distilled from `docs/OCTOCODE_RESEARCH_MANIFEST.md` (verified live against the running toolset — all 13 tools, minification, pagination, bridges); read that doc for the full measurements, worked examples, and related-work comparison.

## Router — route by what you hold, not a fixed pipeline

Running grep first when you hold a symbol name wastes a hop; running LSP first when you hold only a concept cannot work at all.

| What you hold | First move |
|---|---|
| Nothing + a wiki/doc artifact | Read it for named entry points; treat the result as a lead, then verify specific claims. |
| Nothing, no doc artifact | Tree depth 1-2 plus count-matches-per-file on the domain term; re-enter with the hotspot map. |
| Concept or behavior words | Synonym-regex search, then `minify:"symbols"` on the top file for anchors. |
| Identifier | LSP workspaceSymbol, then callers/callees for callables or references for other symbols. |
| Code shape | Structural search with a rule; metavars become typed extraction. |
| Package name | Inspect `node_modules` first; use npmSearch only to find the source repo. |
| Why/history question | Search PR titles or commit history on the path. |
| Binary/archive/huge artifact | Inspect or list before extracting; use strings for leads. |

## Three dimensions — read at least two before concluding

Codebase behavior, impact, or reachability claims can be read through three angles; each alone builds a confidently partial model with no signal that it is partial:

| Dimension | Answers | Tools | Blind to |
|---|---|---|---|
| **structure** | where things live, size, naming | tree, find-files | meaning — a plausibly-named file proves nothing about behavior |
| **stream** | what is actually written | grep, exact reads, symbols outline | identity — cannot tell a call from a comment or same-named symbol |
| **connections** | what refers to what | LSP, structural AST | capability gaps — dynamic dispatch, scripts, unsupported languages are invisible |

Before any nontrivial code conclusion, pull at least two dimensions.
A mismatch between dimensions is a signal to look closer, not noise. One extra tree or symbols call costs less than a wrong "unused" or "impact is X" claim.
For docs, skill, or process claims, use the equivalent trio: location/structure, exact text, and source-of-truth/provenance.

## Evidence grades — never conclude from one grade

| Grade | Source | Trust | Blind to |
|---|---|---|---|
| **semantic** | LSP (definitions, references, callers) | proven identity — project-scoped | scripts, re-exports-as-text, strings, docs |
| **structural** | AST match, metavar ranges | proven shape, exact captures | anything outside the matched node |
| **lexical** | grep, rows pre-classified `declaration/callsite/import/comment` | total coverage | proves nothing about identity |
| **provider** | GitHub search index | weakest — default-branch only | unindexed/archived repos → false zeros |

**Non-negotiable, before any "impact is X" / "unused" / "only used in Y" claim:** diff one package-wide grep against the LSP result.
Include tests, scripts, and configs. Every lexical hit LSP missed is a finding: re-export, shadow copy, string/SQL/config reference, or doc.

## Triangulate claims — batch angles, not just files

Every tool takes up to 5 queries per call. Spend the batch by question type:

- **Location lookup** ("where is X?") → 5 independent probes across files/paths/surfaces.
- **Claim** ("X is unused / never reassigned / always guarded") → batch 2-3 angles on the SAME target.
  Use lexical coverage, structural shape-proof, and semantic identity when the claim depends on symbol identity. A second angle catches false negatives from a guessed pattern.

## Reads: matchString first

`matchString` returns merged slices plus `matchRanges[]` anchors that feed LSP `lineHint` directly.
Default read priority: **matchString > line ranges > fullContent** for small files only.

Quote/diff/edit only from `minify:"none"`.
`standard` mode is lossy: it rewrites quote style and strips comments.
`symbols` mode is orientation-only: signatures, constants with values, and line-gutter anchors.

## node_modules first

The installed dependency is already on disk, is the exact version that runs, and searches in milliseconds. GitHub's default branch may be newer, older, or restructured relative to what's installed.

```
Question about a dependency's behavior?
  1. tree node_modules/<pkg>              — what shipped (dist? src? types?)
  2. search path:node_modules/<pkg>       — excludeDir: [] + noIgnore: true REQUIRED
  3. read the hit                         — .d.ts and shipped src are gold
  4. LSP hover/definition often resolves INTO node_modules types for free
  5. ONLY IF unshipped (git history, tests): npmSearch → repo → external loop
```

`excludeDir: []` is mandatory. Default exclusions silently skip `node_modules`, so "no matches" there means "didn't look."
Watch for dual hits such as `src/` plus `dist/`; prefer the file your resolver actually loads when semantics matter.

## Bridges: local ↔ external

Materialize when you need AST, structural search, LSP, multi-file regex, or a 3rd+ read into one remote area.
One bridge call converts remote code to local-grade evidence: structural rules, native LSP, and matchString all run on the result.

## Schemas: pay for the contract only when about to use it

Read the tool catalog before any single tool's schema. Read one full schema only right before calling a raw tool.
Read `search --scheme --compact` before hand-writing OQL JSON. A guessed field costs a round-trip or a silently ignored parameter.

## Anti-patterns — each one costs real round-trips

| # | Anti-pattern | Fix |
|---|---|---|
| 1 | Fixed pipeline (grep→AST→LSP as a law) | Enter where your knowledge already points (router above) |
| 2 | Concluding impact from one evidence lane | The lexical∩semantic diff is the deliverable, not a nicety |
| 3 | Trusting GitHub search zeros | Default-branch-only + unindexed repos = false-absence machine |
| 4 | Reading before mapping | `minify:"symbols"` first; bodies only for slices that matter |
| 5 | Skipping node_modules | You debug the installed version, not the default branch |
| 6 | Guessing `lineHint` / computing `charOffset` | Anchors come from prior results; cursors come from `pagination.*` |
| 7 | Staying remote too long | 3+ reads into one remote area ⇒ materialize |
| 8 | Ignoring `next.*` hints | They are prefilled, correct continuation queries |
| 9 | Quoting from `minify:"standard"` output | Rewrites quotes, strips comments — quote only from `minify:"none"` |
| 10 | Serial single queries | Up to 5 queries per call, per tool — batch independent probes |
| 11 | Calling a raw tool with guessed field names | Read the schema tier first (see Schemas above) |
| 12 | Spending one angle on a claim | Claims get 2-3 batched angles; a wrong-shape 0 reads like a proven negative |

## Failure signals

| Signal | Meaning | Move |
|---|---|---|
| `status:"empty"` + stats (`filesSearched`, `bytesSearched`) | proven negative *for that scope* | widen scope/synonyms/filters; quote the stats before concluding |
| `status:"error"` + `errorCode` + hint | typed failure; hint names the recovery tool | follow the hint — do not retry verbatim |
| structural 0 matches | usually an incomplete pattern, not absence | add `$$$BODY`/return type, or switch to a rule |
| LSP `serverUnavailable`/`unsupported` | capability absence, NOT "no usage" | fall back to grep / symbols view |
| LSP `completeness.complete:false` | truncated by depth/dynamic-call exclusion | deepen or supplement with grep before claiming full impact |
| gh empty / `providerUnindexed` | index blind spot, NOT absence | verify path exists → materialize → grep locally |
| `resolvedBranch` ≠ requested | ref fell back to default branch | re-check which branch you're actually reading |
| `warnings[]` | redaction, fallback engine, pre-filter skips, pagination notes | read them — they change what the result means |

## What this toolset deliberately doesn't do

No embeddings/vector index and no precomputed knowledge graph.
With a concrete handle such as identifier, filename, error string, or shape, deterministic search can reach proof-grade evidence with zero index setup.
An indexed lane helps most on purely conceptual queries over a very large, unfamiliar codebase where synonym-regex fans out too wide.
Fallback path: use the "nothing" router branch with tree, hotspot map, and symbols skeletons. It is slower, but fresh and evidence-graded.
Full position, including wiki/AutoWiki docs as leads-never-proof: `docs/OCTOCODE_RESEARCH_MANIFEST.md` §1b/§1c.
