# Agent Algorithms — Octocode Research Playbook

Evidence-based decision trees for agents using the Octocode toolset (MCP or CLI).
Every rule in this doc was verified live against the running tools (2026-07-07):
all 13 tools exercised, minification modes measured, every pagination cursor family
followed end-to-end, and the local↔external bridges tested. Where a behavior is
config-gated or has a known gap, it is flagged inline.

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

---

## 3. THE ROUTER (master decision tree)

Route by **what you already hold**, not by a fixed pipeline order.
Running grep first when you hold a symbol name wastes a hop; running LSP first
when you hold only a concept cannot work at all.

```
WHAT DO I HOLD?
│
├─ Nothing (unfamiliar codebase)
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

## 5. LOCAL algorithm (the loop, in full)

```
0. ORIENT     (skip if you know the area)
   localViewStructure         — shape of the directory
   countMatchesPerFile        — which files carry the concern (measured: instant map,
                                e.g. db-usage: db.ts 30, maintenance.ts 29, memory.ts 20)

1. LOCATE     (router entry, §3)

2. MAP        localGetFileContent minify:"symbols" on the winning file.
              Measured: 956-line file → skeleton at ~10× compression that kept
              constants WITH VALUES and every signature with line numbers.
              Those line numbers are ready LSP anchors. This one call often
              answers concept questions outright.

3. PROVE      lspGetSemantics from a real anchor (grep line / symbols line):
              - callers/callees/callHierarchy for callables (impact analysis)
              - references includeDeclaration:false, groupByFile:true for the rest
              - lineHint SELF-CORRECTS (verified: passed 261, resolved to 263 and
                reported foundAtLine) — but never guess it from nothing
              - READ the completeness block: truncatedByDepth, dynamicCallsExcluded,
                stdlibCallsExcluded, failedRequestCount tell you what you did NOT see

4. EXTRACT    localSearchCode mode:"structural" when you need the complete node or
              typed captures. Verified: `db.exec($$$SQL)` + has regex "ALTER TABLE"
              returned each FULL multiline SQL statement as $SQL — grep only sees
              one line of a 30-line template literal.
              Rules of the mode:
              - a `pattern` must match a COMPLETE AST node (body, return type, all
                required syntax). Partial shape → 0 matches (pattern queries get a
                guidance warning; rule queries currently do not).
              - method calls ≠ plain calls: `foo($$$A)` will not match `x.foo($$$A)`;
                write `$RECV.foo($$$A)`.
              - for partial/relational matches use a YAML rule (kind/has/inside/not/any,
                stopBy: end). Bare rule YAML and `rule:`-wrapped are both accepted
                (engine ≥ 2026-07-07; older engines require the `rule:` wrapper).
              - `$$$LIST` captures currently include comma separators as elements — filter.

5. CROSS-CHECK  (non-negotiable before "impact is X" / "unused" / "only used in Y")
              One package-wide grep of the symbol INCLUDING tests/scripts/configs.
              Diff lexical hits vs semantic hits. Every lexical hit LSP didn't report
              is a finding: re-export, shadow copy, string/SQL/config reference, doc.
              (Verified: this exact step exposed a diverged duplicate scorer in a
              skills/*.mjs script that LSP callers could not see.)

6. READ       localGetFileContent with startLine/endLine from matchRanges.
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
                3.6 KB line-gutter skeleton) — orient before pulling bodies
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

---

## 8. LOCAL ↔ EXTERNAL bridges

### External → Local (materialize, then analyze)

| Bridge | How | Gate |
|---|---|---|
| Subtree fetch | `ghGetFileContent type:"directory"` → returns `localPath` + prefilled `next.localSearch`/`next.viewStructure` | `ENABLE_CLONE=true` (off → typed error telling you exactly that) |
| Clone | `ghCloneRepo` (sparse via `sparsePath`) | same gate |
| OQL | `materialize: "auto"/"required"`, or follow a result's `next.materialize` | works per-plan; bounded by budget controls |

**When to materialize:** you need AST/structural, LSP, multi-file regex, or you're
about to make a 3rd+ read call into the same remote area. One materialization
converts every later step from provider-grade to local-grade evidence.

**After materializing:** pass `localPath` into local tools verbatim. The full §5
loop — including LSP — runs on materialized code.

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

---

## 10. Pagination — cursor families (all verified)

**Golden rule: cursors are OPAQUE. Copy them from the response
(`pagination.nextCharOffset`, `nextPage`, `next.*` prefilled queries) — never compute
your own.** Every paginator here is lossless; nothing is silently dropped.

| Family | Fields | Tools | Verified behavior |
|---|---|---|---|
| Char window (file) | `charOffset`/`charLength` → `nextCharOffset`, `isPartial` | local/gh GetFileContent, binary extract/decompress | page 2 resumed mid-token exactly at offset 2000; `next.continueChars` was copy-paste ready |
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
| structural 0 matches | usually an incomplete pattern, NOT absence | add `$$$BODY`/return type, or switch to a rule; check the guidance warning |
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

---

## Appendix: verification log (2026-07-07)

| Claim | How verified |
|---|---|
| All 13 tools functional | one realistic query each, plus error/empty paths |
| symbols minify on both surfaces | local 956-line TS → skeleton; GitHub 12.7 KB → 3.6 KB |
| standard minify ratio + lossiness | 35,260 → 20,524 chars; quote-style rewrites observed |
| char pagination lossless | page 2 resumed mid-token at exact offset; `next.continueChars` ready |
| gh search pagination | page 2 of 26 matches; reported/reachable/capped fields |
| LSP anchor self-correction | lineHint 261 → resolved 263, `foundAtLine` reported |
| LSP completeness honesty | truncatedByDepth / dynamicCallsExcluded / stdlibCallsExcluded:5 |
| lexical-lane necessity | grep found diverged `.mjs` duplicate + re-export that LSP callers missed |
| structural extraction | `db.exec($$$SQL)` + has-regex returned full 30-line SQL as `$SQL` |
| method-call vs call shape | `foo($$$A)` 0 matches → `$RECV.foo($$$A)` 2 matches, same code |
| node_modules-first | zod `coerce` source found in 16 ms with `excludeDir:[]` |
| materialization gate | `type:"directory"` without `ENABLE_CLONE=true` → typed, instructive error |
| OQL federated lane | GitHub rows: `proofGrade:"text"`, `answerReady:false`, `next.fetch` upgrade |
| typed error recovery | nonexistent path → `fileAccessFailed` + "use localFindFiles" |
| empty-as-proof | `status:"empty"` carried filesSearched/bytesSearched stats |
