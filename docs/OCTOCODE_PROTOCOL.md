# The Octocode protocol

**Octocode lets an agent see code from several dimensions and query each one precisely.** Text, files, syntax, symbols, connections, history, packages and semantic judgment are all available. So is every hop between them: from search results down to the exact lines, back out to the surrounding structure, and from GitHub or a package registry into local, semantic analysis.

This page explains how the protocol works and why each part exists. It covers the flow, the logic and the main concepts, then shows where Octocode measurably beats plain tools and where it doesn't. For field-level detail, follow the links to the owner docs.

- [1. The idea: many dimensions, one precise question at a time](#1-the-idea-many-dimensions-one-precise-question-at-a-time)
- [2. The research loop: zoom out, zoom in, prove](#2-the-research-loop-zoom-out-zoom-in-prove)
- [3. The protocol, part by part](#3-the-protocol-part-by-part)
- [4. Where Octocode excels (measured)](#4-where-octocode-excels-measured)
- [5. Where plain tools still win](#5-where-plain-tools-still-win)
- [6. Concept → code map for developers](#6-concept--code-map-for-developers)

---

## 1. The idea: many dimensions, one precise question at a time

A coding agent's scarcest resource is context. Dumping files, whole diffs or raw API responses spends it on noise, and plain tools give no signal about what was truncated, hidden or skipped. Octocode inverts that:

1. **Several views of the same code.** Each view answers a different kind of question, and every view returns citable anchors (path, line, SHA, id).
2. **Ask precisely.** Filters, match windows, symbol names and patch filters make one call return the answer instead of a haystack.
3. **Every result says what to do next.** Executable `next.*` continuations point to the cheapest follow-up, and failures carry repair hints.
4. **Evidence, not guesses.** Every result is pinned, bounded, honest about completeness and scrubbed of secrets.

| Dimension | Question it answers | Tools |
|---|---|---|
| **Content** (text) | Where does this text or pattern occur? What do these exact lines say? | `localSearch`, `localFetch`, `ghSearchCode`, `ghGetFileContent` |
| **Files** (layout) | Which files and directories exist, how big are they, and where should I look? | `structureSearch`, `ghStructure`, `ghSearchRepo` |
| **Syntax** (AST) | Which declarations or code shapes exist, ignoring comments and strings? | `astSearch` (match, symbols, syntax tree), `astRewrite` (beta, CLI) |
| **Semantics** (LSP) | Is this the same symbol? Where is it defined, who references or calls it? | `lspSearch` |
| **Connections** (graph) | Who imports this file? What does it depend on? Is it reachable? | `astTopology` (beta), LSP call hierarchy |
| **History** | Which change introduced this, what did the PR do, what differs between two refs? | `ghSearchHistory`, `ghGetHistoryItem` |
| **Packages** | Which repository and directory hold this package's source? | `artifactSearch` |
| **Judgment** (optional) | Which of these candidates matters, where in the file is the answer, and is this snippet enough? | `clasify` |

## 2. The research loop: zoom out, zoom in, prove

```mermaid
flowchart LR
  S[SCOPE<br/>local or remote, which repo/ref] --> O[ORIENT - zoom out<br/>tree, outline, PR summary]
  O --> F[SEARCH<br/>text, AST, filtered PR query]
  F -->|behavioral target, 2+ candidate files| C[clasify locate/scout]
  F --> R[READ EXACT - zoom in<br/>match window, line range, SHA]
  C --> R
  R --> P[PROVE<br/>LSP identity, diff, references]
  P --> D[DECIDE and cite]
  R -. next.* followUp .-> F
  P -. zoom out .-> O
```

- **Zoom out** before searching blind. `structureSearch` tree/files, `astSearch` symbols (an outline without bodies), `localFetch minify:"symbols"`, `ghStructure`, and a PR summary with counts all show the shape at low cost.
- **Search** with the narrowest filter that settles the question: `langType`, `path`, `include`/`exclude`, `wholeWord`, `regex:"literal"`, AST patterns, PR `fileFilter`/`matchString`.
- **Zoom in** on exact regions. Use `matchString` + `contextLines`, `startLine`/`endLine`, PR `matchContext`, or byte windows for minified files. Whole files are read only when completeness matters and the file is small.
- **Prove** identity semantically with `lspSearch` definition, references and calls. Text hits show occurrence; LSP shows identity.
- **Stop** as soon as the evidence answers the question. One decisive region settles one fact.

**External → local is one continuous flow:**
1. `artifactSearch` maps a package to its repo and directory.
2. `ghSearchRepo`, `ghStructure` and `ghSearchCode` locate the code.
3. `ghGetFileContent` reads it at a pinned SHA.
4. `ghSearchHistory` and `ghGetHistoryItem` explain how it changed.
5. The CLI's `ghCloneRepo` materializes the repo locally, and then every local dimension applies to it: AST, LSP and topology.

## 3. The protocol, part by part

### 3.1 One contract for every surface
- Each tool's input schema, description and output schema is authored once in `@octocodeai/octocode-core` (Zod).
- `@octocodeai/config` generates the JSON contract and the Rust/TS types, and the native runtime embeds them.
- The MCP server and the CLI run the same native code, so their output is **byte-identical**.
- If the core and native contract fingerprints ever differ, the MCP server **refuses to start** rather than serve mismatched schemas.
- Why: agents learn one shape per tool, and drift is caught before it reaches a user.

### 3.2 Goal and reasoning on every query
- Every new query states a short `goal` (what it must find or decide) and a `reasoning` (why this call advances it).
- The runtime rejects a new query without them.
- `next.*` continuations carry `followUp: true` instead and inherit the brief, so it is never repeated.
- Why:
  - **Forced intent.** The agent must say why it is calling, which prevents aimless calls.
  - **Semantic judgment.** The brief gives clasify's judgment the intent it needs.
  - **Audit trail.** Every call in a session is explained by its own row.
- See the [data contract](TOOL_DATA_CONTRACT.md#requests-and-result-rows).

### 3.3 Context engineering: instructions, descriptions, schemas
Each layer is budgeted, and tests enforce the budgets:
- **Server instructions (MCP):**
  - The research loop.
  - The PR rule: ask the PR directly with `matchString`/`matchContext`/`fileFilter`, and use the inventory only when there's no literal or path.
  - The clasify use/skip rule.
  - The evidence rules and the surface map.
  - The whole set is capped at **2,000 characters**, because hosts truncate longer instructions. They are scoped to the tools actually available: without a clasify key, clasify is never mentioned.
- **Tool descriptions** are written as triggers: *use when…, not when…, continue with…*. An agent picks the right tool from the description alone.
- **Schemas** stay lean: descriptions only where a field isn't self-explanatory, enums instead of prose, and examples that validate. `octocode scheme <tool>` shows the full contract on demand, so agents load it only after choosing a tool.

### 3.4 Batching
- One call carries **1–5 independent queries** of the same tool, each with its own goal and reasoning.
- The rows run concurrently. Results keep input order, and a failing row is isolated: its siblings still succeed.
- Batch independent probes (e.g. three candidate files, two synonyms). Keep dependent steps sequential.

### 3.5 Minimal responses by default
- Results carry the answer plus what's needed to continue: `next`, open pagination, warnings and partial-coverage signals.
- Scan statistics, provider receipts, snapshots and info-level notes appear only with `debug: true`.
- Text output drops redundant wrappers: a single-row response has no `results`/`index`/`data` nesting, and path-only rows render as `path` or `path (count)`.
- A contract guard restores any field the output contract requires, so minimizing never breaks a response.

### 3.6 Minification
- **Modes:**
  - `minify:"none"` gives source text exactly (the default for reads).
  - `"standard"` compacts content: comments, blank runs, and minified-bundle clipping.
  - `"symbols"` gives a whole-file outline of signatures and headings, without bodies.
- **Strategies** are chosen per format: code, JSON, Markdown, web. Minified single-line files are clipped to windows around matches, with `truncated` and the original size reported.
- **Rule:** positions in a transformed view are **not** source lines. Agents cite `none` reads or search hits, and the docs and instructions say so.

### 3.7 Smart pagination everywhere
Every list and every large body can be paged, and every page is honest about what remains:
- **Item pages:** files, matches, symbols, PR files, commits, comments.
- **Content windows:** character or line windows inside a large file or patch. A huge file costs one bounded first page (e.g. 18k chars for a 3.2 MB file) instead of the whole file.
- **Whole-response paging:** `responseCharOffset`/`responseCharLength`, used when rendered output itself is large.
- **Snapshots and cursors:**
  - Pages are bound to a snapshot, so page 2 matches page 1's view. When the source changes, a `restart` is offered.
  - Cursors are HMAC-keyed, so they can't be forged.
- **Honest totals:**
  - `hasMore`, `totalPages`, and `countScope:"unknown"` when totals aren't known.
  - Provider caps are reported, never hidden: GitHub's 300-file compare limit and 3000-file PR limit, and the 100-file limit of `gh pr view`.

### 3.8 Next steps and failure hints
- Every result can carry `next.*`: an **executable** query to replay unchanged (next page, read the top match, open the repo, read the full patch, run clasify on these candidates). Next steps the current surface can't run are removed.
- **Empty results** come with repair hints: widen the scope, try a synonym, check the ref or index limits. An empty result is never an absence claim until scope, spelling, ref and index limits have been checked.
- **Errors** are typed (`invalidInput`, `notFound`, `authentication`, `rateLimited`, …), carry `retryable`, and map to distinct CLI exit codes:
  - 0 success;
  - 1 empty;
  - 2 invalid input;
  - 3 not found;
  - 4 authentication;
  - 5 execution, configuration or availability error;
  - 6 partial, meaning there is more to read;
  - 7 rate limited.
- A renamed repository is followed automatically: `next.retryRenamed` points at the new owner.

### 3.9 Precision layers: AST and LSP
- **AST (tree-sitter), 12 grammars / 28 extensions:**
  - Structural patterns (`$X.unwrap()`, `new Error($MSG)`) and YAML rules match code shape, never comments or strings.
  - Symbols give exact declaration inventories.
  - `astRewrite` applies codemods only after a preview, and only if the files are unchanged since that preview.
- **LSP:**
  - Real language servers (tsserver, rust-analyzer, pyright, clangd, …) resolve definitions, references, call hierarchy, hover and diagnostics.
  - Coordinates are 1-based source positions taken from a real anchor.
  - A missing server gives a clean `lsp.serverUnavailable` error plus a text-search next step.
- **Ranking:**
  - Search hits inside a file are ranked declaration > deciding statement (assignment, `if`, `return`, `throw`) > code > comment.
  - For an exact identifier, the file that declares it ranks first.

### 3.10 clasify: semantic judgment before reading
- `clasify` (optional; needs a classification API key) judges **unread** candidates:
  - **Scout** rates each file or item from a list tool's results.
  - **Locate** finds the line window that answers a question.
  - **Judge** answers typed questions: contribution, supportsClaim, addsEvidence, and `sufficient`, which asks whether a snippet already answers the question.
- It returns verdicts and windows, **never file bodies**. Each result carries the exact `next.read` for what's worth opening.
- The provider receives the evidence plus goal, reasoning and question instructions, never tokens, cursors or snapshots.
- Judgments are cached per process, keyed on the full state (content + question + goal), and identical in-flight calls are merged. Errors are never cached.
- **When it pays:** there is a behavioral target (no exact literal) and there are two or more candidate files. **Skip it** for identifiers, literals and PR filters, where exact search already settles the question.
- **Without a key** it disappears entirely: from the tool list, the instructions, and every `next.*`.
- See [OCTOCODE_CLASIFY.md](OCTOCODE_CLASIFY.md).

### 3.11 Security: sanitize input and output
- **Input:**
  - The contract is validated before execution: unknown fields are rejected and nothing is silently clamped.
  - Regexes and structural patterns are validated.
  - Local paths stay inside the sandbox (the workspace root, `ALLOWED_PATHS`, `OCTOCODE_HOME`). Symlink and `..` escapes are denied, and sensitive files (`.env`, keys, credential stores) are refused even inside allowed roots.
  - Directory walks skip dependency, build and cache folders by default (`defaultExcludes`).
- **Output:**
  - Every byte bound for the model is scanned by the native secret scanner. It covers cloud, AI-provider, VCS, registry, database, payment and private-key formats, including PEM blocks split across page windows.
  - Values are replaced with typed markers and a warning.
  - Searches never match inside a secret, so a match can't reveal it.
  - Email masking is opt-in.
- **Egress:** clasify is the only tool that sends content to a third party. It is key-gated, and its payload excludes credentials and cursors.
- See [SECURITY.md](SECURITY.md).

### 3.12 Local caching and GitHub rate limits
- GitHub file bodies and directory listings are cached in memory and, with persistent storage, on disk. The disk cache is shared by the CLI and MCP.
- Entries stay fresh for 5 minutes. Listings keep their ETag for conditional refresh, which returns 304 when unchanged, and bodies read at a commit SHA never need revalidating.
- Rate limits are respected, never hammered:
  - `retryAfterSeconds` and `resetEpochSeconds` are reported.
  - A local circuit breaker refuses further calls ("request not sent") until the window resets, instead of burning the quota.
- clasify judgments have their own process-local cache (section 3.10).
- See [CONFIGURATION.md](CONFIGURATION.md#cache-storage-and-lifecycle).

### 3.13 The Rust layer: local efficiency
- A single native engine runs everything local, with nothing extra to install:
  - search (ripgrep libraries);
  - parsing and structural matching (tree-sitter);
  - minification;
  - redaction;
  - the LSP client pool;
  - the dependency graph;
  - GitHub and registry clients.
- **One engine, two entry points:** MCP loads it in-process through napi, so calls are warm (about 10–20 ms of overhead). The CLI is a native binary.
- **Bounded parallel work:** concurrent rows share one walk-thread budget (peak threads went from 72 to 22 at equal speed), with parse size limits and deadlines on rewrites.

## 4. Where Octocode excels (measured)

All numbers come from real runs against independently verified ground truth, recorded in [BENCHMARKS.md](BENCHMARKS.md).

| Claim | Evidence |
|---|---|
| Less context than an expert `gh` user on large PRs | 9.6k vs 45k chars over 4 PRs (37–656 files), all answers correct |
| Correct where the obvious `gh` commands fail | `gh pr view --json files` silently stops at 100 files, and `gh pr diff` fails with 406 above 300. Octocode filters a 656-file PR in one call and flags the files GitHub sent without a patch |
| Honest completeness | It reports GitHub's 300-file compare cap (true count 339) and follows renamed repositories where `gh` silently returns 0 |
| Reproducible evidence | Reads return `commitSha`, continuations stay pinned to it, and PRs carry `sourceSha`/`mergeCommitSha` |
| Semantic precision | LSP references are exact; `rg -w` precision on the same symbols was 0.26–0.62 |
| Syntax-aware accuracy | Symbols were exact on 6 of 6 files (regexes/ctags scored 0.81–0.98). Codemods were identical to ast-grep on 5 of 5, while sed was wrong on 4 of 5 |
| Safe on hostile input | 0 of 5 fake secrets leaked, vs 3 of 3 for rg. A 3.2 MB file costs 18k chars (vs 3.1M), and a 2 MB minified line costs 710 (vs 2.07M) |
| clasify on "how/where" questions | Whole-file locate: 10/10 vs 9/10 for rg, with 43% fewer files opened |
| Local parity with rg | Same accuracy (14/15). About 8% more chars, spent on the citable line gutter |
| Degrades cleanly | Without a clasify key: 12 tools, no mentions, no dangling next steps |

## 5. Where plain tools still win

- **Per-call latency:** rg answers in 3–25 ms, and the Octocode CLI starts in about 200 ms per call. MCP is warm; LSP cold starts take seconds.
- **Whole-list GitHub reads:** a full compare, a deep tree or a long issue thread takes several paged calls, where a single `gh api --jq` pipeline takes one.
- **Tiny single-line answers:** bare `gh` or rg output beats Octocode's evidence (SHA, line numbers, pagination) by a few hundred characters.
- **Scouting PRs with clasify:** a literal filter is 16× cheaper, so the instructions tell agents to skip clasify there.

## 6. Concept → code map for developers

| Concept | Where |
|---|---|
| Contracts, descriptions, instructions, budgets | core `src/toolContract/` (`instructions.ts`, `descriptions.ts`, `validation/`, `outputSchemas.ts`, `limits.ts`) → `packages/octocode-config/contract/` |
| MCP startup, fingerprint gate, registration | `packages/octocode-mcp/src/native/index.ts` |
| CLI dispatch and exit codes | `packages/octocode-native/crates/cli/src/cli/mod.rs` |
| Validation, the brief rule, row isolation | `crates/runtime/src/contracts/` (`validate.rs`, `mod.rs`) |
| Gate, dispatch, row shaping | `crates/runtime/src/runtime/engine.rs`, `domain_dispatch.rs` |
| Minimal output, next-step filtering | `crates/runtime/src/runtime/response.rs` |
| followUp continuations | `crates/runtime/src/runtime/continuations.rs` |
| Rendering and response paging | `crates/runtime/src/runtime/render.rs`, `response_stage.rs`, `crates/runtime/src/response/mod.rs` |
| Cursors | `crates/runtime/src/runtime/cursor.rs` |
| Path sandbox and directory pruning | `crates/runtime/src/policy/path.rs`, `policy/prune.rs` |
| Secret scanning and redaction | `crates/engine/src/security/`, `crates/runtime/src/security/content.rs` |
| Search and ranking | `crates/engine/src/search/` (`ripgrep_search.rs`, `relevance.rs`) |
| Minification | `crates/engine/src/minify/` |
| AST and rewrite | `crates/engine/src/structural/`, `crates/runtime/src/tools/ast_*` |
| LSP | `crates/engine/src/lsp/`, `crates/runtime/src/tools/lsp_search/` |
| Dependency graph | `crates/engine/src/graph/`, `crates/runtime/src/tools/ast_graph/` |
| GitHub client, rate-limit budget, cache | `crates/github/src/` (`budget.rs`: rate-limit breaker), `crates/runtime/src/runtime/github.rs`, `github_cache.rs` |
| clasify | `crates/runtime/src/runtime/clasify_*.rs`, `crates/runtime/src/tools/clasify/` (including `cache.rs`) |
| Benchmark and harness | `octocode-local-testing/bench/`, `octocode-local-testing/harness/` |

**Owner docs:**
- [Research manifest](OCTOCODE_RESEARCH_MANIFEST.md): choosing and combining tools.
- [Tools reference](OCTOCODE_TOOLS.md): every field.
- [Data contract](TOOL_DATA_CONTRACT.md): rows, continuations, evidence boundaries.
- [clasify](OCTOCODE_CLASIFY.md).
- [Security](SECURITY.md).
- [Configuration](CONFIGURATION.md) and [Authentication](AUTHENTICATION.md).
- [MCP](OCTOCODE_MCP.md) and [CLI](../packages/octocode/docs/OCTOCODE_CLI.md).
- [Development](../skills-dev/octocode-dev/docs/DEVELOPMENT.md).
