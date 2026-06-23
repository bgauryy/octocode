# Agent Research Workflows

> Product guidance for agent-facing Octocode workflows: combine package search,
> GitHub code/repo search, fetch, clone, local ripgrep, AST structural search,
> and LSP semantics into one evidence-first research loop.

## Core Rule

Use layered retrieval, not one magic search path:

```text
orient -> search -> fetch exact evidence -> prove -> act
```

Carry anchors forward at every step: package names, owner/repo, branch, path,
line, match text, PR number, localPath, symbol name, and LSP lineHint.

## Hard Rules For Agents

These rules are mandatory when an agent is constructing Octocode calls.

1. Read the active schema before any raw tool call:
   `octocode tools <name> --scheme`, then `octocode tools <name> --queries '<json>'`.
   Field names differ between quick commands and raw tools — e.g. quick `cat`
   uses `--mode none|standard|symbols` while the raw tool field is `minify`
   (`--minify` is rejected on the quick command).
2. Use `--json` for automation and `--compact` for low-token exploration. Human
   rendering is useful for quick inspection, but raw envelopes carry pagination,
   diagnostics, hints, and continuations.
3. Treat search results as candidate evidence. Prove claims with fetched file
   content, exact PR/patch content, local paths, LSP results, binary metadata, or
   tests.
4. Follow returned `next.*`, `hints[]`, `pagination`, `charOffset`, `matchPage`,
   `filePage`, `commentPage`, and `commitPage` data. Do not invent offsets,
   pages, local paths, or branches.
5. Never call zero results proof until spelling, branch/ref, path, language,
   filters, provider limitations, pagination, auth, and rate limits are checked.
6. Use local/materialized proof for predicates GitHub providers cannot evaluate
   exactly: structural AST, PCRE2-only regex, negative file queries, file metadata,
   LSP semantics, binary/archive inspection, and many-file repeated reads.
7. Batch independent raw-tool queries up to the active schema limit; serialize
   dependent steps that need returned anchors.
8. Do not hide capability gaps. If a target is partial, use the quick command or
   raw tool fallback and say which evidence surface was used.

## Surface Selection

Choose the surface by the job, not by habit.

| Surface | Use when | Required agent behavior |
|---------|----------|-------------------------|
| Quick commands | The workflow is common and expressible as CLI flags (`grep`, `cat`, `ls`, `find`, `lsp`, `pr`, `history`, `binary`, `unzip`, `clone`, `cache fetch`) | Prefer `--json` when another step depends on the result; preserve returned paths, refs, and pagination. |
| OQL `search` | One typed query should route across active OQL targets (code/content/files/structure, semantics, repo/package/history, artifacts, diff, research, graph, materialize), or an agent needs a normalized research plan via `--explain` | Use `search --scheme`; use `--explain` when routing/completeness is uncertain; follow `next.*` when present. |
| Raw `tools` | A quick command or OQL cannot express the needed field, pagination domain, content selector, or exact target behavior | Always run `tools <name> --scheme` first; pass schema-exact JSON only. |
| Direct local shell | Only for repo maintenance around Octocode itself, not for agent-facing research flows | Prefer Octocode CLI/MCP tools for research so behavior stays dogfooded. |

## OQL Coverage And Fallbacks

OQL is the preferred typed research object for its strong surfaces, but agents must
know where the current language is still partial.

| Need | Prefer OQL? | Fallback when OQL is partial |
|------|-------------|------------------------------|
| Local/GitHub text, regex, structural code search | Yes | Raw `localSearchCode` / `ghSearchCode` when a field is not modeled. |
| Exact local/GitHub content reads | Yes | Raw `localGetFileContent` / `ghGetFileContent` for unusual pagination or match options. |
| Local/GitHub file discovery and tree structure | Yes | Quick `find` / `ls` or raw local/GitHub tools for renderer or metadata gaps. |
| Remote-as-local proof | Yes | Use `target:"materialize"` for bounded GitHub checkpoints; fall back to `cache fetch`, `clone`, or `ghCloneRepo` when clone is disabled or a CLI cache workflow is more ergonomic. |
| LSP semantics | Yes | OQL `target:"semantics"` covers document symbols, navigation, workspace symbols, type hierarchy, and diagnostics; quick `lsp`, `ls --symbols`, or raw `lspGetSemantics` are still useful for one-off terminal workflows. |
| Packages and repositories | Yes | `pkg`, `repo`, raw `npmSearch`, or raw `ghSearchRepos` are quicker when a typed OQL envelope or continuation is not needed. |
| PRs, commits, and history | Mostly | Use `target:"pullRequests"`, `target:"commits"`, and `target:"diff"`; fall back to `pr`, `history`, or raw `ghHistoryResearch` for highly specific review/comment/patch pagination. |
| Artifacts, archives, binaries | Mostly | Use `target:"artifacts"` for inspect/list/extract/decompress/strings/unpack; use `binary`/`unzip` or raw `localBinaryInspect` when a manual unpack-to-local flow is clearer. |
| Diffs | Yes | Use OQL `target:"diff"` for PR patch lanes and direct `{baseRef, headRef, path}` file/ref diffs; quick `diff` remains convenient for terminal comparisons. |
| Dead code, reachability, unused files, package drift | Partly | Start with OQL `target:"research"` for a repo-level candidate flow; confirm destructive cleanup with LSP references, AST import search, exact reads, and/or knip. |
| Relationship graph, retained-by chains, missing proof | Partly | Use OQL `target:"graph"` after bounding a local/materialized corpus; follow packet `next.semantic` / `next.fetch` before deletion claims. |
| Structural metavariable captures | Yes | OQL structural code rows expose `metavars`/`metavarRanges` when the backing structural search returns captures; quick/raw structural search remains useful for tuning patterns. |

## Diagnostic And Failure Handling

Agents should report diagnostics as evidence about completeness, not as noise.

| Signal | Meaning | Next step |
|--------|---------|-----------|
| `auth` / token error | GitHub/npm/private data may be inaccessible | Check `status`; ask for auth only if the task requires protected data. |
| `rate limited` | Provider result is incomplete for now | Preserve query and retry later or narrow scope. |
| `ENABLE_LOCAL` / local disabled | Local filesystem, clone, directory fetch, LSP, or binary work may be blocked | Use remote-only proof where possible; otherwise enable local tools. |
| `ENABLE_CLONE` / clone disabled | Materialization and directory fetch are unavailable | Use `ghGetFileContent` slices or ask to enable clone for local proof. |
| `serverUnavailable` / LSP unavailable | Semantic proof is inconclusive | Use AST/exact content evidence; retry after materializing project context or installing the server. |
| `partialResult`, truncation, `hasMore`, or char pagination | The response is not complete | Follow the advertised continuation before concluding. |
| Sanitizer/redaction warning | Secret-like content was masked | Do not reconstruct the secret; cite only non-sensitive evidence. |
| Provider approximation | The provider did not prove every predicate | Materialize and re-run locally, or downgrade the claim to candidate evidence. |
| Empty provider result | Could be true absence or bad scope | Verify ref/path/spelling/filters and try structure/read/materialization before concluding. |
| Cache hit/stale cache | Local evidence may reflect cached remote content | Use `--force-refresh` only when freshness matters. |

## AST/LSP Research Graph Flow

Use this flow for dead-code, reachability, retained-by, and safe-delete
questions. Text search can start the investigation, but proof must come from AST,
LSP, and graph reasoning.

```text
Structure
  -> repo shape, package roots, manifests, source dirs, tests, generated dirs

Discovery
  -> ripgrep patterns, file predicates, dynamic strings, candidate anchors

AST inventory
  -> declarations, imports, exports, calls, class/function structure

LSP proof
  -> documentSymbols, references, definition, typeDefinition,
     implementation, callers, callees, callHierarchy,
     workspaceSymbol, supertypes, subtypes, diagnostic

Graph reasoning
  -> entrypoint reachability
  -> internal-only export detection
  -> transitive dead-code pruning
  -> retainedBy / why / missingProof

OQL packet
  -> agent-readable answer + exact next inspection
```

Language tiers:

| Tier | Capability | Agent rule |
|---|---|---|
| Tier 1 | Structure + AST + LSP + graph | Best proof. Bound the corpus, inventory with AST, prove with LSP, reason over the graph. |
| Tier 2 | Structure + AST + graph | Good structural candidate evidence. Mark missing semantic proof. |
| Tier 3 | Structure + ripgrep/file search | Discovery only. Never treat as deletion proof. |

Full contract:
https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OQL_RESEARCH_GRAPH_FLOW.md

## Product Guidance For Octocode

Octocode should teach agents this decision ladder:

| Need | Start with | Prove with | Avoid |
|------|------------|------------|-------|
| Known package | `npmSearch` / CLI `pkg` | source repo + exact file reads | guessing GitHub repo names |
| Unknown repo | `ghSearchRepos` / CLI `repo` | `ghViewRepoStructure`, README, examples | cloning before scope is known |
| Known remote symbol/string | `ghSearchCode` | `ghGetFileContent` line range or `matchString` | treating snippets as proof |
| One remote file | `ghGetFileContent(type:"file")` | exact minify mode, line range, or match slice | full clone |
| One remote directory | `ghGetFileContent(type:"directory")` | returned `localPath` + local tools | many one-file API reads |
| Deep remote analysis | `ghCloneRepo` | local ripgrep + AST + LSP | GitHub-only semantic claims |
| Local lexical search | `localSearchCode` | `localGetFileContent` exact slice | reading whole files first |
| Code shape | `localSearchCode(mode:"structural")` | AST matches plus exact slices | regex over comments/strings |
| Symbol identity | `lspGetSemantics` | definition/references/callers/callees | LSP without a search anchor |
| Why code changed | `ghHistoryResearch` | direct `prNumber` metadata, files, patches | broad PR comment search first |
| Unified multi-domain query | CLI `search` / OQL | routed backing tool evidence | guessing raw tool fields |
| Dead-code/package-drift sweep | `search target:"research"` | AST inventory + LSP proof + graph reachability packets | deleting from text/ripgrep or heuristic counts alone |
| Relationship/keeper chain | `search target:"graph"` | nodes, edges, facts, packets, missing proof, and exact next inspection | reading whole files before using graph filters |
| Remote content cache | CLI `cache fetch` | returned `localPath` + local tools | repeated remote file reads |
| Repository tree shape | `ghViewRepoStructure` / CLI `ls` | targeted file reads | path guessing |
| Local metadata/path search | `localFindFiles` / CLI `find` | exact file slices | content search for filenames only |
| Local directory shape | `localViewStructure` / CLI `ls` | find/search/read follow-ups | recursive whole-file reads |
| Archive or binary artifact | `localBinaryInspect` / CLI `binary` or `unzip` | archive listing, unpacked `localPath`, nested binary inspection, local searches | treating opaque assets as plain text |
| Diff or selected patch | CLI `diff` or `ghHistoryResearch` selected patches | exact patch/file slice | full PR dumps |

## CLI Command Map

Quick commands are the fastest path for humans and agents. Raw `tools` calls are
schema-exact and should be used when the quick command cannot express the
needed field.

| CLI command | Backing workflow | Use when |
|-------------|------------------|----------|
| `search` | OQL router across local, GitHub, npm, PRs, commits, artifacts, diff, and smart research flows | One typed query should choose the backing tool, emit a plan, and return continuations or typed research rows |
| `pkg` | `npmSearch` | Starting from an npm package or package keyword |
| `repo` | `ghSearchRepos` | Discovering candidate repositories |
| `ls` | `localViewStructure` or `ghViewRepoStructure` | Orienting by tree shape or symbol outline |
| `grep` | `localSearchCode` or `ghSearchCode` | Searching text, regex, paths, or local AST patterns |
| `cat` | `localGetFileContent` or `ghGetFileContent` | Fetching exact file evidence |
| `find` | `localFindFiles` or remote file discovery | Locating files by name, path, metadata, or content |
| `lsp` | `lspGetSemantics` | Definitions, references, call hierarchy, hover, type info |
| `pr` | `ghHistoryResearch` | PR search or direct PR inspection |
| `history` | `ghHistoryResearch(type:"commits")` | Commit history for a repo, directory, or file |
| `clone` | `ghCloneRepo` | Full repo or sparse subtree materialization |
| `cache fetch` | clone/file/tree materialization lane | Saving remote content locally for local tools |
| `binary` | `localBinaryInspect` | Inspecting binaries, listing/extracting archives, decompressing streams, or reading strings |
| `unzip` | `localBinaryInspect(mode:"unpack")` | Unpacking archives, then continuing with `ls`/`find`/`grep`/`cat`/`lsp` and `binary` for nested artifacts |
| `diff` | direct file diff or PR patch workflow | Comparing files or inspecting selected changes |

Before using `tools <name> --queries`, read `tools <name> --scheme`. Before using
`search --query`, read `search --scheme` or run with `--explain` when unsure.

### Remote-As-Local Shortcut (`--repo`)

`grep`, `find`, `cat`, and `ls` accept `--repo <owner/repo[@ref]>`. This
materializes the repo (or subpath) under `.octocode` in one step, runs the local
tool against the saved files, and returns the absolute local path in `location`.
It is the fastest way to get local-only power (ripgrep controls, structural AST,
file metadata, symbol outlines) on a remote repo without a separate `clone` or
`cache fetch` step.

```bash
octocode grep "registerTool" --repo facebook/react packages/react --json --compact
octocode grep --repo facebook/react packages/react --pattern 'useMemo($$$ARGS)' --json
octocode find "*.test.ts" --repo owner/repo --json
octocode cat src/index.ts --repo owner/repo@main --mode none --json
```

With `--repo`, `arg[0]`/the path argument is repository-relative. Reuse the
returned absolute `location` path with plain local `ls`/`grep`/`cat`/`lsp` for
follow-up calls — the files stay materialized. Remote structural search
(`--pattern`/`--rule` against GitHub) **requires** `--repo`; GitHub's code-search
API cannot evaluate AST predicates.

## Best Workflows

### 1. Package To Source To Evidence

Use this when the user names a library, npm package, or framework feature.

```text
npmSearch(packageName)
-> take repository owner/repo from package metadata
-> ghViewRepoStructure(owner, repo, path="", depth=1)
-> ghSearchCode(owner, repo, keywordsToSearch=[distinctive symbol])
-> ghGetFileContent(owner, repo, path, matchString/startLine/endLine)
-> ghCloneRepo only if local AST/LSP proof is needed
```

CLI equivalent:

```bash
octocode pkg <package>
octocode ls <owner/repo>
octocode grep <symbol> <owner/repo>
octocode cat <owner/repo/path> --match-string <symbol> --mode none --json
octocode clone <owner/repo[/subpath]>
```

### 2. Repo Discovery To Pattern Examples

Use this when the agent needs prior art, examples, or comparable
implementations across GitHub.

```text
ghSearchRepos(keywordsToSearch, topicsToSearch?, language?, stars?)
-> shortlist active/relevant repositories
-> ghSearchCode(owner, repo, keywordsToSearch, extension/path filters)
-> ghGetFileContent for exact examples
-> ghHistoryResearch when the reason or evolution matters
```

Start with `ghSearchRepos`; use `ghSearchCode` after you know which repositories
or owners are worth reading. This keeps broad GitHub search from becoming noisy.

### 3. GitHub Code Search To Fetch

Use this when a remote repo is known and the agent needs exact evidence.

```text
ghSearchCode(owner, repo, keywordsToSearch, extension?, path?)
-> read result status and pagination
-> ghGetFileContent(owner, repo, path, matchString or line range)
-> cite the fetched file slice, not the search snippet
```

Rules:

- Use distinctive identifiers and scope by owner/repo as soon as possible.
- If search is empty, verify spelling, branch, path, extension, and filters before
  calling it absence.
- Fetch one file or slice before cloning.

### 4. Remote To Local Bridge

Use this when remote research needs local-only power: ripgrep controls,
structural search, file metadata, binary inspection, or LSP.

```text
ghViewRepoStructure(owner, repo, path, depth)
-> choose materialization:
   - ghGetFileContent(type:"directory") for one small directory
   - ghCloneRepo for full project, sparse subtree, AST, or LSP
   - CLI `--repo owner/repo[@ref]` on grep/find/cat/ls for one-command
     materialize-and-search (returns the saved absolute path in `location`)
-> localViewStructure(localPath)
-> localSearchCode(localPath)
-> localSearchCode(mode:"structural") when shape matters
-> lspGetSemantics(uri, symbolName, lineHint) when identity matters
```

Clone only when the clone cost buys something: project context, many files,
AST/LSP proof, call tracing, or repeated local searches.

### 5. Local Repo Investigation

Use this inside a checked-out workspace or cloned repo.

```text
localViewStructure(path, recursive=true, maxDepth=1)
-> localFindFiles(path, names/pathPattern/modifiedWithin)
-> localSearchCode(path, keywords, mode:"discovery" or filesOnly=true)
-> localGetFileContent(path, matchString/startLine/endLine)
-> localSearchCode(mode:"structural", pattern/rule) for syntax-aware proof
-> lspGetSemantics(type, uri, symbolName, lineHint) for semantic proof
```

Use ripgrep-style search for cheap anchors. Use AST when the claim is about code
shape. Use LSP after a line anchor exists.

### 6. Change History And Intent

Use this when the user asks why a behavior exists or when a regression may come
from a prior PR.

```text
ghSearchCode(owner, repo, keywordsToSearch)
-> ghHistoryResearch(owner, repo, query, matchScope=["title","body"])
-> ghHistoryResearch(owner, repo, prNumber, content={metadata, changedFiles})
-> selected patches/comments only when needed
-> clone + local tools if current code needs semantic follow-up
```

Prefer title/body PR search before broad comment search. When a PR number is
known, jump directly to `prNumber`.

### 7. Unified Search Router

Use this when an agent should express the research target once and let Octocode
route to the right backing tool.

```text
search(target:"packages", from:{kind:"npm"}, params:{packageName})
-> search(target:"repositories", from:{kind:"github"}, params:{keywords:[term]})
-> search(target:"code", from:{kind:"github" or "local"}, where:{kind:"text"|"regex"|"structural"})
-> search(target:"content", fetch:{content:{...}})
-> search(target:"research", from:{kind:"local"|"materialized"}, params:{goal, mode})
-> follow next.* continuations or rerun with --explain when routing is unclear
```

Repository OQL params follow the raw `ghSearchRepos` shape: `keywords` and
`topicsToSearch` are arrays even for one term. Field predicates use symbolic
operators such as `op:"="`, `op:"glob"`, and `op:"regex"`; aliases like
`op:"eq"` are rejected by the schema.

Use raw tools instead when the agent needs a tool-specific field, exact pagination
control, or a schema feature not yet modeled by OQL.

Executable CLI pattern:

```bash
octocode search --scheme
octocode search --query '{"target":"code","from":{"kind":"local","path":"src"},"where":{"kind":"text","value":"registerTool"},"view":"discovery","limit":10,"explain":true}' --json
octocode search --query '{"target":"content","from":{"kind":"local","path":"src/index.ts"},"fetch":{"content":{"match":{"text":"registerTool"},"contentView":"exact"}}}' --json
octocode search --query '{"target":"research","from":{"kind":"local","path":"."},"params":{"goal":"find unused exports, transitive dead code, unused files, and package drift","mode":"analyze"}}' --json
```

Use `--explain --dry-run` first when the target, materialization strategy, or
predicate pushdown is uncertain.

### 8. Cache Fetch To Local Proof

Use this when remote code should become local evidence without committing to a
full manual clone workflow. This is a CLI-only convenience surface over the
underlying GitHub materialization tools: it checks existing local tmp/cache
materialization first and uses `--force-refresh` only when the cache must be
bypassed.

```text
cache fetch owner/repo path --depth file|tree|clone
-> cache hit? reuse localPath; otherwise materialize into tmp/tree or tmp/clone
-> capture localPath, repoRoot, cached, complete, and resolvedBranch
-> localViewStructure(localPath)
-> localFindFiles(localPath)
-> localSearchCode(localPath)
-> localGetFileContent(localPath + file)
-> lspGetSemantics(uri under repoRoot) when project context is present
```

Use `--depth file` for a single file, `--depth tree` for a directory, and
`--depth clone` for project-wide local search or LSP. With no explicit depth,
the CLI defaults to `file` when a path is supplied and `clone` when no path is
supplied.

Executable CLI pattern:

```bash
octocode cache fetch owner/repo path/to/dir --depth tree --json
octocode ls /absolute/localPath --json
octocode grep "symbolName" /absolute/localPath --json --compact
octocode cat /absolute/localPath/file.ts --match-string "symbolName" --mode none --json
```

If freshness matters, rerun `cache fetch` with `--force-refresh` and record that
the evidence came from a refreshed local path.

### 9. Artifacts, Archives, And Binaries

Use this when useful code or evidence is inside a compressed artifact, archive,
or native binary.

```text
localBinaryInspect(mode:"inspect") for native metadata
-> localBinaryInspect(mode:"list") before extracting archives
-> localBinaryInspect(mode:"strings") for printable identifiers, URLs, symbols
-> localBinaryInspect(mode:"extract") for one archive entry
-> localBinaryInspect(mode:"unpack") or CLI unzip for multi-file archive research
-> localViewStructure/localFindFiles/localSearchCode over returned localPath
-> localGetFileContent for exact text evidence
-> localBinaryInspect again for nested .node/.so/.dll/.wasm/.zip artifacts
```

List before extract. Use `strings` to find anchors. Use `unzip` when the next
step is normal local research across many unpacked files; use `binary --extract`
when only one archive member matters.

`--json` callers: `strings` output lands in `data.content` (char-paginated via
`charOffset`, with `totalFound`/`scanOffset`), not a `strings[]` array. Read
`data.content` and follow the char cursor for the rest.

### 10. Unzip To Binary And Local Search

Use this when an archive may contain source, generated bundles, nested archives,
or native binaries.

```text
unzip artifact.zip
-> capture localPath
-> ls localPath
-> find localPath for likely source, manifests, nested archives, and binaries
-> grep localPath for identifiers, secrets, URLs, errors, imports, or config keys
-> cat exact matching files or slices
-> binary nested-artifact for inspect/list/strings/decompress/extract
-> lsp localPath/file when unpacked source has project context
```

This turns opaque release artifacts into normal local research surfaces. Search
the unpacked tree first, then inspect nested binary/archive files only when a
path or string anchor makes them relevant.

Executable CLI pattern:

```bash
octocode unzip artifact.zip --json
octocode ls /absolute/unpacked/localPath --json
octocode find /absolute/unpacked/localPath "*.js" --json
octocode grep "apiKey|endpoint|register" /absolute/unpacked/localPath --perl-regex --json
octocode binary /absolute/unpacked/localPath/native.node --json
```

OQL artifact extract/unpack/materialize rows should emit local follow-up
continuations when they return a usable `localPath`. If a binary mode only
returns an entry list, string page, or metadata payload, continue manually from
the returned path/entry fields.

### 11. Diff And Patch Review

Use this when the question is about what changed between two files, refs, or PR
states.

```text
diff(left, right) for direct file comparison
-> ghHistoryResearch(prNumber, content={metadata, changedFiles})
-> ghHistoryResearch(prNumber, content={patches:{mode:"selected", files:[...]}})
-> ghGetFileContent or localGetFileContent for current exact source
```

Prefer selected patch files or ranges over full PR patches. Use the current file
content to separate "what changed" from "what exists now".

For PR diffs, prefer selected files. For direct file/ref diffs, OQL supports the
`{baseRef, headRef, path}` lane; the CLI `diff` surface remains convenient when
the comparison is purely terminal-facing.

### 12. Smart Reachability, Unused Symbols, And Package Drift

Use this when the question is whether exports, files, dependencies, or symbols
are still reachable. Start broad with Smart OQL, then prove risky rows with
semantic and structural evidence.

```text
search(target:"research", from:{kind:"local"|"materialized"},
  params:{goal:"find unused exports, transitive dead code, unused files, and package drift",
          mode:"plan"|"analyze"})
-> inspect data.flow for the planned evidence chain
-> inspect data.packets, data.graphSummary, packetPage, and missingProof
-> when the question is "what keeps this alive?", switch to target:"graph"
-> because evidence.kind is candidate, prove destructive edits with LSP/AST/exact reads
```

Executable CLI pattern:

```bash
octocode search --query '{"target":"research","from":{"kind":"local","path":"."},"params":{"goal":"find unused exports, transitive dead code, unused files, and package drift","mode":"analyze"}}' --json
octocode search --query '{"target":"graph","from":{"kind":"local","path":"."},"params":{"goal":"what keeps candidate-dead exports alive?","intent":"reachability","verdict":["candidate-dead","transitive-dead"],"direction":"incoming","includePackets":true},"itemsPerPage":25}' --json
octocode search --query '{"target":"research","from":{"kind":"local","path":"."},"params":{"goal":"how should an agent prove unused exports?","mode":"plan"}}' --json
```

Current behavior is intentionally conservative: `target:"research"` returns
candidate evidence. It builds a file/import/manifest graph and identifies
zero-use, transitive-dead, unused-file, unlisted-dependency, unused-dependency,
and duplicate-dependency candidates, but framework entrypoints, build outputs,
TS path aliases, dynamic imports, generated files, and package-manager-specific
workspace rules still require refinement. Treat large count deltas from knip as
signal that the project graph needs better entrypoint/materialization context,
not as deletion instructions.

Use manual LSP/AST proof for the final symbol decision:

```text
lspGetSemantics(uri, type:"documentSymbols")        # enumerate symbols + exact lines
-> for each candidate, lspGetSemantics(type:"references", symbolName, lineHint,
     includeDeclaration:false, groupByFile:true)
-> zero references (totalReferences == 0) => unused candidate
-> prove with exact content read; check for transitive/same-file-only references
```

`documentSymbols` gives the exact `line` to feed back as the `lineHint` for the
`references` call. `includeDeclaration:false` excludes the definition so the count
reflects real callers. This is candidate evidence, not a final verdict: a symbol
referenced only from other dead code, or only within its own file, still reports
references — confirm reachability from a real entry point before deleting.

CLI equivalent:

```bash
octocode ls ./src/module.ts --symbols --json
octocode lsp ./src/module.ts --type references --symbol mySymbol --line 42 --json
```

Observed comparison in this repo after enabling `target:"research"`:
the previous `packages/octocode/src/configs/mcp-registry.ts` sample improved
from 6/7 agreement against knip's unused-export verdict to 7/7, including the
transitively-dead `MCPCategory` type. Whole-repo agreement is useful but still
candidate-grade: the Smart OQL pass overlapped most knip files/symbols, while
over-reporting until entrypoint and workspace semantics are made more precise.

### 13. LSP-Compatible Evidence Graph For Research Questions

Use this when the agent needs to answer repo research questions without reading
the whole repository: "What looks dead?", "Why?", "What keeps it alive?", "Is
that keeper itself dead?", "What proof is missing?", "What exact file/line
should I inspect next?", or "Is this safe to delete?"

The design rule: use LSP concepts for identity and proof. Subjects and edges
should carry `uri`, `range`, `SymbolKind`, definitions, references,
implementations, and call hierarchy where available. AST and ripgrep are still
first-class, but they should feed the same evidence graph instead of inventing a
separate symbol model.

| Layer | Use it for | Evidence status |
|---|---|---|
| ripgrep text/regex | cheap anchors, broad counts, dynamic/string hints, config name checks | candidate unless exact read confirms |
| structural AST | imports, exports, declarations, class/function shapes, call shapes, syntactic containment | strong syntax fact, not semantic identity |
| signatures/symbols | compact repo inventory and line hints for follow-up LSP | exact-ish declaration inventory |
| LSP | definitions, references, type definitions, implementations, callers, callees, call hierarchy | semantic proof when pages and capabilities are complete |
| exact content read | citations, final inspection, destructive-change proof | exact source proof |

Ask OQL for packets, not raw dumps:

```text
subject, verdict, proofStatus, why, retainedBy, retains, missingProof, risk, next
```

Minimal-ingest flow:

```text
1. inventory files, manifests, source roots, tests, generated/output dirs
2. collect AST/signature facts: declarations, imports, exports, functions, classes, calls
3. build candidate packets from facts and edges
4. expand LSP proof only for selected packets or bounded batches
5. exact-read only the next file/line needed for a citation or deletion decision
```

Question routing:

| Agent question | Preferred path |
|---|---|
| What looks dead? | `target:"research"` with `mode:"analyze"` and facets `symbols/files/dependencies` |
| Why? | inspect packet `why` facts and `missingProof` diagnostics |
| What keeps it alive? | `target:"graph"` with `direction:"incoming"` and optional `subject` / `verdict` filters |
| Is that keeper itself dead? | re-query `target:"graph"` for the retained-by subject with cycle guards, then prove with LSP |
| What proof is missing? | inspect `missingProof`; follow `next.semantic`, `next.search`, or `next.fetch` |
| What exact file/line next? | use packet `subject.uri/range` and `next.fetch` |
| Is this safe to delete? | require no reachable external refs, no high-severity missing proof, and exact source inspection |
| Function/class flow? | prefer LSP `callHierarchy`/`callers`/`callees`; fall back to AST call-shape edges flagged as candidate |

Never treat a graph candidate alone as deletion proof. A safe answer should say
what is known, what retains the subject, what proof is missing, and which exact
file/line to inspect next.

## Exact Agent Recipes

These are copy-pasteable patterns agents can adapt. Replace placeholders, then
keep the returned anchors for follow-up calls.

### Raw Tool Schema-First Call

```bash
octocode tools localSearchCode --scheme
octocode tools localSearchCode --queries '{"path":"packages","keywords":"registerTool","mode":"discovery","maxFiles":20}' --json --compact
octocode tools localGetFileContent --scheme
octocode tools localGetFileContent --queries '{"path":"packages/example/src/index.ts","matchString":"registerTool","contextLines":12,"minify":"none"}' --json
```

### Search Then Exact Read

```bash
octocode grep "createServer" ./packages --json --compact
octocode cat ./packages/foo/src/server.ts --match-string "createServer" --mode none --json
```

### Enumerate Matches In A Minified Or One-Line File

```bash
octocode tools localSearchCode --scheme
octocode tools localSearchCode --queries '{"path":"dist/bundle.js","keywords":"https?://\\S+","perlRegex":true,"onlyMatching":true,"unique":true,"matchWindow":20}' --json
```

### Structural Search With Exact Proof

```bash
octocode grep ./src --pattern 'eval($X)' --type ts --json
octocode grep ./src --pattern 'registerTool($$$ARGS)' --type ts --json   # captures call args
octocode grep ./src --rule $'rule:\n  kind: function_item' --type rust --json  # all defs
octocode cat ./src/example.ts --start-line 40 --end-line 70 --mode none --json
```

A `$$$ARGS` (or `$X`) capture returns the matched text per row under
`matches[].metavars`, so call-site argument enumeration comes back as data, not
just line numbers. Use a `kind:` YAML rule to match every definition of a node
type (e.g. `function_item`, `class_declaration`). Use YAML `rule` for relational
AST logic (`inside`/`has`/`not`/`all`/`any`). In shells, pass real newlines using
a file, stdin, or ANSI-C quoting; do not rely on literal `\n` text becoming
newlines, and single-quote patterns so `$` metavariables survive the shell.

Gotcha: in typed languages, the pattern must match the real syntax. An
`export function $N($$$ARGS): $RET { $$$BODY }` matches TS exports only when a
return-type annotation is present; the bare `export function $N($$$ARGS) { $$$BODY }`
returns zero matches against annotated declarations.

### LSP After A Search Anchor

```bash
octocode grep "MySymbol" ./src --json --compact
octocode lsp ./src/file.ts --type definition --symbol MySymbol --line 42 --json
octocode lsp ./src/file.ts --type documentSymbols --json
octocode search --query '{"target":"semantics","from":{"kind":"local","path":"./src/file.ts"},"params":{"type":"documentSymbols"}}' --json
octocode tools lspGetSemantics --scheme
octocode tools lspGetSemantics --queries '{"uri":"/absolute/path/src/file.ts","type":"documentSymbols"}' --json
```

Use quick `lsp` for common local navigation and `documentSymbols`; use OQL
`target:"semantics"` when the result should stay inside a typed OQL envelope;
use raw `lspGetSemantics` when a schema-specific semantic type or pagination
field is easier to express directly. Use `ls --symbols` for quick symbol
outlines when a semantic LSP outline is not required.

### PR Metadata To Selected Patch

```bash
octocode pr owner/repo --query "routing bug" --state merged --json
octocode pr owner/repo#1234 --json
octocode tools ghHistoryResearch --scheme
octocode tools ghHistoryResearch --queries '{"owner":"owner","repo":"repo","prNumber":1234,"content":{"changedFiles":true}}' --json
octocode tools ghHistoryResearch --queries '{"owner":"owner","repo":"repo","prNumber":1234,"content":{"patches":{"mode":"selected","files":["src/router.ts"]}}}' --json
```

### Package To Repo Fallback

```bash
octocode pkg <package> --json
octocode repo <package> --language TypeScript --json
octocode ls owner/repo --json
octocode grep "<distinctive symbol>" owner/repo --json --compact
```

If npm metadata is missing or stale, prove the source by reading `package.json`
from the candidate repository.

## Evidence Gates

- Search snippets are discovery, not proof. Fetch exact source before making a
  claim.
- Empty status is not absence until scope, spelling, branch, filters, and
  pagination are checked.
- AST proves syntax shape, not runtime behavior or types.
- LSP proves semantic identity when the server is available and configured; an
  empty or unavailable server is inconclusive.
- Semantic or embedding search is useful for conceptual discovery, but every hit
  must be proved through exact file content, AST, LSP, PR history, or tests.
- Use pagination and match windows before expanding scope.
- Batch independent queries; serialize dependent steps that rely on returned
  anchors.
- If OQL returns generic records or missing continuations for research targets, switch
  to the quick command or raw tool and preserve that fallback in the final
  evidence trail.
- If structural `metavars` are absent from OQL output, use exact snippets/lines
  or quick/raw structural search evidence rather than fabricating captures.

## Workflow Defaults For Agents

| Phase | Default behavior |
|-------|------------------|
| First pass | `concise:true`, path-only, `mode:"discovery"`, shallow tree depth |
| Reading | `matchString`, line ranges, or `minify:"symbols"` before whole files |
| Local search | literal/fixed-string search before broad regex |
| Structural search | `pattern` for simple shapes; YAML `rule` for relational logic |
| LSP | search first, then pass `uri`, `symbolName`, and `lineHint` |
| Remote research | package/repo/code search first; clone only for local proof |
| Materialization | `cache fetch` or `ghCloneRepo`; capture `localPath` and continue locally |
| Artifacts | inspect/list/strings first; `unzip`/unpack before local search; run `binary` again on nested artifacts |
| Diffs | direct `diff` for file comparison; selected PR patches for review history |
| Reporting | cite fetched files, PRs, package metadata, or exact local paths/lines |

## Completeness Checklist

Before answering from Octocode research, confirm:

1. The corpus is explicit: local path, package, owner/repo, branch/ref, PR number,
   artifact path, or materialized `localPath`.
2. The query surface is justified: quick command, OQL, or raw tool.
3. Raw-tool fields came from the active `--scheme`.
4. Candidate results were converted into exact evidence.
5. Pagination and continuations were followed or declared unnecessary.
6. Diagnostics and provider limitations were handled.
7. Claims distinguish syntax proof, semantic proof, history proof, binary proof,
   and runtime/test proof.
8. Fallbacks are named when OQL or a quick command was not expressive enough.

## References

Internal Octocode references:

- [GitHub Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/GITHUB_TOOLS.md)
- [Local Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LOCAL_TOOLS.md)
- [LSP Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LSP_TOOLS.md)
- [Clone & Local Tools Workflow](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CLONE_WORKFLOW.md)
- [Tool Behavior Guide](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/TOOL_BEHAVIOR.md)
- [CLI Reference](https://github.com/bgauryy/octocode/blob/main/docs/cli/REFERENCE.md)
- [Binary Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/BINARY_TOOLS.md)
- [Octocode Query Language](https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_QUERY_LANGUAGE.md)

External references:

- [GitHub Code Search syntax](https://docs.github.com/en/search-github/github-code-search/understanding-github-code-search-syntax)
- [GitHub repository search](https://docs.github.com/en/search-github/searching-on-github/searching-for-repositories)
- [npm package search guidance](https://docs.npmjs.com/searching-for-and-choosing-packages-to-download/)
- [ripgrep](https://github.com/BurntSushi/ripgrep)
- [ast-grep AI prompting](https://ast-grep.github.io/advanced/prompting.html)
- [Tree-sitter introduction](https://tree-sitter.github.io/tree-sitter/)
- [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
