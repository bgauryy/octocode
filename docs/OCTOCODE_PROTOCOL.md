# The Octocode protocol

**Octocode lets an agent see code from several dimensions and query each one precisely.** Text, files, syntax, symbols, connections, history, packages, and semantic judgment are all available, and so is every hop between them: from search results to exact lines, back out to the surrounding structure, and from GitHub or a package registry into local analysis.

This page owns the concept and why each part exists. Routing and flows: [OCTOCODE_WORKFLOWS.md](OCTOCODE_WORKFLOWS.md). Fields: [OCTOCODE_TOOLS.md](OCTOCODE_TOOLS.md). Envelope, pages, and leads: [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md).

## 1. The idea: many dimensions, one precise question at a time

Context is a coding agent's scarcest resource. Whole files, whole diffs, and raw API responses spend it on noise, and plain tools do not say what they cut, hid, or skipped. Octocode inverts that:

1. **Several views of the same code.** Each view answers a different question and returns citable anchors (path, line, SHA, id).
2. **Precise questions.** Filters, match windows, symbol names, and patch filters make one call return the answer, not a haystack.
3. **Every result says what to do next.** `next.*` pages reach the rest of a result, `hints` leads point to the cheapest optional follow-up, and failures carry repair tips.
4. **Evidence, not guesses.** Every result is pinned, bounded, honest about completeness, and scrubbed of secrets.

| Dimension | Question it answers | Tools |
|---|---|---|
| **Content** (text) | Where does this text occur? What do these exact lines say? | `localSearch`, `localFetch`, `ghSearchCode`, `ghGetFileContent` |
| **Files** (layout) | Which files exist, how big are they, where should I look? | `structureSearch`, `ghStructure`, `ghSearchRepo` |
| **Syntax** (AST) | Which declarations or code shapes exist, ignoring comments and strings? | `astSearch` |
| **Semantics** (LSP) | Is this the same symbol? Where is it defined, who references or calls it? | `lspSearch` |
| **Connections** (graph) | Who imports this file? What does it depend on? Is it reachable? | `octocode graph query` (CLI), LSP call hierarchy |
| **History** | Which change introduced this, what did the PR do, what differs between two refs? | `ghSearchHistory`, `ghGetHistoryItem` |
| **Packages** | Which repository and directory hold this package's source? | `artifactSearch` |
| **Judgment** (optional) | Which captured hits or passages are relevant; a typed decision about held or unread source | `clasifyContext` on search/fetch, `clasify` (beta) |

## 2. The research loop: zoom out, zoom in, prove

```mermaid
flowchart LR
  S[SCOPE<br/>local or remote, which repo/ref] --> O[ORIENT - zoom out<br/>tree, outline, PR summary]
  O --> F[SEARCH<br/>text, AST, filtered PR query]
  F -->|typed decision over unread source| C[clasify]
  F --> R[READ EXACT - zoom in<br/>match window, line range, SHA]
  C --> R
  R --> P[PROVE<br/>LSP identity, diff, references]
  P --> D[DECIDE and cite]
  R -. next.* page / hints suggestion .-> F
  P -. zoom out .-> O
```

- **Zoom out** before you search blind: a tree, a symbols outline, `minify:"symbols"`, or a PR summary shows the shape at low cost.
- **Search** with the narrowest filter that settles the question.
- **Zoom in** on exact regions: `matchString` windows, `ranges`, PR `contextLines`, or byte windows for minified files. Read a whole file only when it is small and completeness matters.
- **Prove** identity with `lspSearch`. Text hits show occurrence; LSP shows identity.
- **Stop** when the evidence answers the question.

External to local is one flow: `artifactSearch` maps a package to its repository, the GitHub tools locate and read the code at a pinned SHA, the history tools explain how it changed, and a clone or `materialize` brings it local for AST, LSP, and topology. Each flow: [OCTOCODE_WORKFLOWS.md](OCTOCODE_WORKFLOWS.md).

## 3. The protocol, part by part

### 3.1 One contract for every surface
- Each tool's input schema, description, and output schema is authored once in `@octocodeai/octocode-core` (Zod). `@octocodeai/config` generates the JSON contract and the Rust and TypeScript types, and the native runtime embeds them.
- MCP and the CLI share native tool execution and structured result data; their text presentation can differ.
- If the core and native contract fingerprints differ, the MCP server refuses to start.
- Zod is the authoring source; generated JSON contracts and Rust/TypeScript types carry it to consumers. To keep discovery compact, `tools/list` shows a slim view derived in Zod from the same contract (`publishedQuerySchema`): merged operation variants, the primary fields, and no fields agents copy from `next.*` and `hints`. JSON Schema is emitted once at each edge (`z.toJSONSchema`: MCP `tools/list`, the CLI presentation, the native contract). The canonical schema still validates every call, and its errors list every valid field. `npx octocode schema` shows it.
- Why: agents learn one shape per tool, and drift is caught before it reaches a user.

### 3.2 Optional brief for multi-call research
- `mainGoal` (the research question) and `reasoning` (why this call advances it) are optional on every tool. A blank brief is dropped.
- Why: a lookup spends nothing on a brief and cannot fail for a missing one; a research call explains itself in its own row. When `clasifyContext` is present, the supplied brief also accompanies its atomic questions to JEV and is bound into selection replay.
- Rules: [OCTOCODE_WORKFLOWS.md](OCTOCODE_WORKFLOWS.md#briefs-maingoal-and-reasoning).

### 3.3 Context engineering: instructions, descriptions, schemas
- **Instructions:** one prompt for MCP and CLI, one line per section, at most 2,000 characters as a project instruction-size budget; the `<flow>` line states the whole research loop. Tool-specific instruction parts follow the enabled tool set; `<clasify>` also follows classification availability. The lines: [OCTOCODE_WORKFLOWS.md](OCTOCODE_WORKFLOWS.md#research-flow-graph).
- **Tool descriptions** follow one grammar in plain text: prose (what the tool does, when it is right), an optional `If empty:` (the repair when a result is empty or a literal misses), then one `Next:` (where its output goes next). A rule test enforces it; no tool name appears outside `Next:`.
- **Field descriptions** are the one text of each field (its Zod `.describe()`): what the value is, and, for a value another tool's output fills, the output that fills it (`ref`: a search row's `commitSha` or a PR `headSha`; `line`: a hit's line). Self-explanatory fields carry none. The tools connect through their inputs and outputs, so the flow is read where the value is typed.
- A core test caps the bytes of the instructions plus every default tool definition, with a quota per tool.

### 3.4 Batching
- One call carries 1–5 independent queries of the same tool. Rows run concurrently, keep input order, and fail alone. Rules: [OCTOCODE_WORKFLOWS.md](OCTOCODE_WORKFLOWS.md#make-a-call).

### 3.5 Minimal responses by default
- A result carries the answer and what is needed to continue: `next` pages, open pagination, warnings, and partial-coverage signals. A successful row's leads are opt-in; empty and failed rows keep theirs.
- Fields the core output contract classes `extra` appear only when the query's `extras` names their kind: `hints` (leads), `details`, `stats`, `receipts`, `echo`, `diagnostics`. The MCP instructions name `extras` once; it is not in each published schema. A contract guard restores any field the output contract requires, so minimizing never breaks a response.
- Details: [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md#minimal-by-default).

### 3.6 Minification
- Omit `minify` for exact source (the file-read default). `minify:"standard"` drops comments and indentation and compacts blank runs and minified bundles, and `"symbols"` gives an outline of signatures and headings. Strategies follow the format: code, JSON, Markdown, web.
- A minified single-line file is clipped to windows around matches, with `originalChars` and `returnedChars` reported.
- A transformed view keeps source line numbers in its gutter. Cite source or diff lines by number; quote only verbatim text (reads without `minify`, `matchString` reads, search hits).

### 3.7 Smart pagination everywhere
- Every list and every large body pages: item pages, content windows inside a file or patch, and whole-response pages. A huge file costs one bounded first page.
- Pages are bound to a snapshot, a SHA-256 fingerprint that proves a cursor still answers its query, so page 2 matches page 1. When the source changes, the page fails with `staleSnapshot` and offers `next.restart`.
- Totals are honest: `hasMore`, `totalPages`, or `countScope:"unknown"`. Provider caps, such as GitHub's 3,000-file PR limit, are reported, never hidden.

### 3.8 Pages, leads, and failure hints
- Follow-up calls use two channels. Both hold executable queries that run unchanged, and the runtime removes follow-ups the current surface cannot run.
  - **`next.*` = pages and recovery:** follow the pages needed for the claim's scope. `next.unfiltered`, when offered, is optional recovery of the complete view after classification ([context selection](OCTOCODE_TOOLS.md#context-selection)); it does not make a sufficient selected answer incomplete.
  - **`hints` = optional strings:** plain text or `<action>: <tool> <JSON input>` suggestions named by purpose (`read` the top match, `readPullRequest` the fix, `viewTree` the scope); `hints` adds a sentence only for a per-call fact. Each suggestion keeps its complete validated input; duplicate text and calls appear once. A successful row shows leads only with `extras:["hints"]`, except a few default leads (`includeIgnored`, `countFiles`, `apply`, …).
- An empty result keeps its leads. It is not an absence claim until scope, spelling, ref, and index limits are checked; the general repairs are stated once in each tool's description.
- Errors are typed (`invalidInput`, `notFound`, `authentication`, `rateLimited`, …) and carry `retryable: true` only when a retry can help. Each class has its own CLI exit code; see the [CLI guide](OCTOCODE_CLI.md#exit-codes).
- GitHub tools follow a renamed repository to its canonical name and add a warning that names it.
- Channels and fields: [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md#executable-continuations).

### 3.9 Precision layers: AST and LSP
- **AST (tree-sitter):** structural patterns (`$X.unwrap()`, `new Error($MSG)`) and rule objects match code shape, never comments or strings. Symbols give exact declaration inventories. Languages: [language and feature reference](../packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md).
- **LSP:** real language servers (tsgo, typescript-language-server, rust-analyzer, pyright, clangd, …) resolve definitions, references, call hierarchy, hover, and diagnostics from 1-based positions taken from a real anchor. A missing server gives a `serverUnavailable` error and a `findText` lead (a `localSearch` of the name).
- **Ranking:** hits in a file rank declaration > deciding statement (assignment, `if`, `return`, `throw`) > code > comment. For an exact identifier, the file that declares it ranks first.

### 3.10 Classifiers and clasify: selective evidence
- `clasifyContext:{classifiers:[...],longContext?:boolean}` selects evidence on `localFetch`, `localSearch`, `ghGetFileContent` and `ghSearchCode` before it reaches the host. Optional `mainGoal` and `reasoning` add task context. Search evaluates captured matches; fetch evaluates source chunks. Both use the same evaluator, cache and provider as public `clasify`.
- The default body contains only selected evidence in source order. The `selection` receipt counts selected, excluded and unknown units within its declared scope. Recovery of omitted evidence, and what a provider or source failure returns instead: [context selection](OCTOCODE_TOOLS.md#context-selection). Selection never proves absence outside that assessed scope.
- Use selection when it can avoid substantial reading. Prefer an adequate exact lookup for small targets. Public `clasify` supplies typed Noul, Choice, Score and Box answers over internally captured text or images, without returning the captured source body.
- A nonblank classification credential enables context selection without another activation flag. Public `clasify` additionally needs beta. Setup, outputs and availability: [classification guide](OCTOCODE_CLASIFY.md#availability).

### 3.11 Security: sanitize input and output
- **Input:** the contract is validated before execution; unknown fields are rejected and nothing is silently clamped. Local paths stay inside the sandbox, and sensitive files are refused even inside allowed roots.
- **Output:** the native secret scanner reads every byte bound for the model, including PEM blocks split across page windows, and replaces values with typed markers. Searches never match inside a secret.
- **Classification egress:** `clasify` and `clasifyContext` send sanitized evidence and questions to the configured classifier. Their payloads exclude credentials and continuation tokens. Details: [SECURITY.md](SECURITY.md#classification-egress).

### 3.12 Local caching and GitHub rate limits
- GitHub file bodies and listings are cached in memory and, with persistent storage, on a disk cache that the CLI and MCP share. Reads at a commit SHA never need revalidation.
- Rate limits are respected: a call waits out a short limit window and fails with `rateLimited` and the retry time when the wait would be longer. Cache: [cache storage](CONFIGURATION.md#cache-storage-and-lifecycle); waits: [fixed internal limits](CONFIGURATION.md#fixed-internal-limits).

### 3.13 The Rust layer: local efficiency
- One native engine runs text search (an in-process regex walker), parsing and structural matching (tree-sitter), minification, redaction, the LSP client pool, the dependency graph, and the GitHub and registry clients. Some LSP operations require an installed language server; see [language servers](OCTOCODE_TOOLS.md#language-servers).
- The CLI and the MCP server are one native binary (`octocode mcp`), so MCP calls are warm: the runtime and its caches live as long as the connection.
- Concurrent rows share one walk-thread budget, with parse size limits and scan deadlines.

## 4. Strengths and limits

Earlier scripted tool-level comparisons measured single tool calls, not agent outcomes. Agent-against-agent comparisons, including where Octocode loses, are run with the [benchmark package](../packages/octocode-benchmark/README.md); their results stay local under `packages/octocode-benchmark/results/`.

| Where Octocode wins | Evidence |
|---|---|
| Large PRs | 9.6k against 45k characters over 4 PRs (37–656 files), all answers correct. `gh pr view --json files` stops at 100 files and `gh pr diff` fails above 300; Octocode filters a 656-file PR in one call and flags files sent without a patch. |
| Honest completeness | It reports GitHub's 300-file compare cap (true count 339) and follows renamed repositories where `gh` returns 0. |
| Reproducible evidence | Reads return `commitSha`, pages stay pinned to it, and PRs carry `headSha` and `mergeCommitSha`. |
| Precision | LSP references are exact (`rg -w`: 0.26–0.62). Symbols were exact on 6 of 6 files (regexes and ctags: 0.81–0.98). |
| Hostile input | 0 of 5 fake secrets leaked (rg: 3 of 3). A 3.2 MB file costs 18k characters, and a 2 MB minified line costs 710. |
| Local parity with rg | Same accuracy (14 of 15), with about 8% more characters for the citable line gutter. |

| Where plain tools win | Why |
|---|---|
| Per-call latency | rg answers in 3–25 ms. A CLI call reuses warm state through `octocode serve` (unless `storage.mode = memory`): about 10 ms warm, 120 ms for a workspace's first call, 100 ms in process (one localSearch, 2026-10-17). On the CLI and MCP alike, a language server's cold start takes seconds. |
| Whole-list GitHub reads | A full compare, a deep tree, or a long thread takes several paged calls; one `gh api --jq` pipeline takes one. |
| Tiny answers | Bare `gh` or rg output is a few hundred characters shorter than Octocode's evidence (SHA, lines, pagination). |
| Agent outcomes | In the agent benchmark, `rg` + `gh` still beat Octocode on quality and cost. |

The code that implements each part: [DEVELOPMENT.md](../skills-dev/octocode-dev/docs/DEVELOPMENT.md#concept-to-code).
