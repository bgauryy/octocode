# Research Algorithm

The router, evidence model, and failure playbook behind `octocode-research`. **This is the single source for these rules** — every other reference in this skill points here instead of restating them. Read this once per session before the mode-specific flows.

Distilled from `docs/AGENT_ALGORITHMS.md` (verified live against the running toolset — all 13 tools, minification, pagination, bridges); read that doc for the full measurements, worked examples, and related-work comparison.

## Router — route by what you hold, not a fixed pipeline

Running grep first when you hold a symbol name wastes a hop; running LSP first when you hold only a concept cannot work at all.

```
WHAT DO I HOLD?
├─ Nothing + a wiki/doc artifact exists (ARCHITECTURE.md, droid-wiki/, openwiki/,
│  .devin/wiki.json, GitHub Wiki, DeepWiki/Code Wiki)
│    → read it for orientation and named entry points — a LEAD, not proof
│    → re-enter this router to verify any specific claim before relying on it
│
├─ Nothing, no doc artifact
│    → tree (maxDepth 1-2) + count-matches-per-file on the domain term → hotspot map
│    → re-enter this router with what you learned
│
├─ A concept / behavior (words, no identifier)
│    → synonym-regex search (e.g. "halfLife|half_life|HALF_LIFE")
│    → minify:"symbols" on the top file → the anchor sheet
│
├─ An identifier (function/class/const)
│    → LSP workspaceSymbol — skip grep for locating
│    → callers/callees (callables) or references groupByFile (everything else)
│
├─ A code shape ("all X calls that do Y")
│    → structural search with a rule — metavars = typed extraction
│
├─ A package name
│    → node_modules FIRST (see below) → npmSearch only to find the repo
│
├─ A "why" / history question
│    → PR search (keywords + match:["title"]) or commit history on the path
│
└─ A binary / archive / huge artifact
     → inspect (list before extract; strings for leads)
```

## Evidence grades — never conclude from one grade

| Grade | Source | Trust | Blind to |
|---|---|---|---|
| **semantic** | LSP (definitions, references, callers) | proven identity — project-scoped | scripts, re-exports-as-text, strings, docs |
| **structural** | AST match, metavar ranges | proven shape, exact captures | anything outside the matched node |
| **lexical** | grep, rows pre-classified `declaration/callsite/import/comment` | total coverage | proves nothing about identity |
| **provider** | GitHub search index | weakest — default-branch only | unindexed/archived repos → false zeros |

**Non-negotiable, before any "impact is X" / "unused" / "only used in Y" claim:** diff one package-wide grep (including tests/scripts/configs) against the LSP result. Every lexical hit LSP didn't report is a finding — re-export, shadow copy, string/SQL/config reference, doc. The disagreement between lanes *is* the finding, not noise to discard.

## Reads: matchString first

`matchString` (local + GitHub) hands back merged slices — not N reads — plus `matchRanges[]`: exact line anchors that feed LSP `lineHint` directly. Default read priority: **matchString > line ranges > fullContent** (small files only).

Quote/diff/edit only from `minify:"none"`. `standard` mode is lossy (rewrites quote style, strips comments — ~0.58x chars). `symbols` mode is orientation-only: signatures + constants with values, ~0.10-0.28x size, line-gutter anchors — never read logic bodies from it.

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

`excludeDir: []` is mandatory — default exclusions silently skip `node_modules`, and "no matches" there means "didn't look." Watch for dual hits (`src/` + `dist/` in the same package) — prefer the one your resolver actually loads when semantics matter.

## Bridges: local ↔ external

Materialize the moment you need AST, structural search, LSP, multi-file regex, or you're about to make a 3rd+ read into the same remote area. One bridge call (file/tree/repo depth, gated `ENABLE_CLONE=true`) converts remote code to full local-grade evidence — structural rules, native LSP, and matchString all run unmodified on the result.

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

No embeddings/vector index and no precomputed knowledge graph. Deterministic lexical/structural/semantic search reaches proof-grade evidence with zero index setup whenever you hold a concrete handle (identifier, filename, error string, shape). The one case an indexed lane would beat this router: purely conceptual queries over a very large, unfamiliar codebase where synonym-regex fans out too wide — there, fall back to the "nothing" router branch (tree + hotspot map + symbols skeletons) and say the search is slower but fresh and evidence-graded. See `docs/AGENT_ALGORITHMS.md` §1b/§1c for the full position (including how to treat an existing wiki/AutoWiki doc as a lead, never as proof).
