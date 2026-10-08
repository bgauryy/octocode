# The Octocode protocol

**Octocode lets an agent see code from several dimensions and query each one precisely.** Text, files, syntax, symbols, connections, history, packages, and semantic judgment are all available, and so is every hop between them: from search results to exact lines, back out to the surrounding structure, and from GitHub or a package registry into local analysis.

This page owns the concept and why each part exists. Routing and flows: [OCTOCODE_WORKFLOWS.md](OCTOCODE_WORKFLOWS.md). Fields: [OCTOCODE_TOOLS.md](OCTOCODE_TOOLS.md). Envelope, pages, and leads: [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md).

## 1. The idea: many dimensions, one precise question at a time

Context is a coding agent's scarcest resource. Whole files, whole diffs, and raw API responses spend it on noise, and plain tools do not say what they cut, hid, or skipped. Octocode inverts that:

1. **Several views of the same code.** Each view answers a different question and returns citable anchors (path, line, SHA, id).
2. **Precise questions.** Filters, match windows, symbol names, and patch filters make one call return the answer, not a haystack.
3. **Every result says what to do next.** `next.*` pages reach the rest of a result, `hints.*` leads point to the cheapest optional follow-up, and failures carry repair tips.
4. **Evidence, not guesses.** Every result is pinned, bounded, honest about completeness, and scrubbed of secrets.

| Dimension | Question it answers | Tools |
|---|---|---|
| **Content** (text) | Where does this text occur? What do these exact lines say? | `localSearch`, `localFetch`, `ghSearchCode`, `ghGetFileContent` |
| **Files** (layout) | Which files exist, how big are they, where should I look? | `structureSearch`, `ghStructure`, `ghSearchRepo` |
| **Syntax** (AST) | Which declarations or code shapes exist, ignoring comments and strings? | `astSearch`, `astRewrite` (beta, CLI) |
| **Semantics** (LSP) | Is this the same symbol? Where is it defined, who references or calls it? | `lspSearch` |
| **Connections** (graph) | Who imports this file? What does it depend on? Is it reachable? | `astTopology` (beta, CLI), LSP call hierarchy |
| **History** | Which change introduced this, what did the PR do, what differs between two refs? | `ghSearchHistory`, `ghGetHistoryItem` |
| **Packages** | Which repository and directory hold this package's source? | `artifactSearch` |
| **Judgment** (optional) | Which candidate matters, where in the file is the answer, is this snippet enough? | `clasify` |

## 2. The research loop: zoom out, zoom in, prove

```mermaid
flowchart LR
  S[SCOPE<br/>local or remote, which repo/ref] --> O[ORIENT - zoom out<br/>tree, outline, PR summary]
  O --> F[SEARCH<br/>text, AST, filtered PR query]
  F -->|list to classify, big known file, absence screen| C[clasify locate/scout]
  F --> R[READ EXACT - zoom in<br/>match window, line range, SHA]
  C --> R
  R --> P[PROVE<br/>LSP identity, diff, references]
  P --> D[DECIDE and cite]
  R -. next.* page / hints.* lead .-> F
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
- MCP and the CLI run the same native code, so their output is the same.
- If the core and native contract fingerprints differ, the MCP server refuses to start.
- Hosts resend every tool definition on every request, so `tools/list` shows a slim view of the same contract (`publishedInputSchema`): merged operation variants, full descriptions only on primary fields, and no validation-only bounds or fields agents copy from `next.*` and `hints.*`. The canonical schema still validates every call, and its errors list every valid field. `npx octocode schema` shows it.
- Why: agents learn one shape per tool, and drift is caught before it reaches a user.

### 3.2 Optional brief for multi-call research
- `mainGoal` (the research question) and `reasoning` (why this call advances it) are optional on every tool. A blank brief is dropped.
- Why: a lookup spends nothing on a brief and cannot fail for a missing one; in research, `mainGoal` gives `clasify` the intent it needs; a research call explains itself in its own row.
- Rules: [OCTOCODE_WORKFLOWS.md](OCTOCODE_WORKFLOWS.md#briefs-maingoal-and-reasoning).

### 3.3 Context engineering: instructions, descriptions, schemas
- **Instructions:** one prompt for MCP and CLI, one line per section, at most 2,000 characters (1,600 bytes) because hosts truncate longer ones. The prompt is the same for every tool subset. The lines: [OCTOCODE_WORKFLOWS.md](OCTOCODE_WORKFLOWS.md#research-flow-graph).
- **Tool descriptions** are triggers: use when, not when, continue with. An agent picks the right tool from the description alone.
- **Schemas** stay lean: descriptions only where a field is not self-explanatory, enums instead of prose, and examples that validate.
- Core tests keep the instructions plus every default tool definition within 15,000 bytes.

### 3.4 Batching
- One call carries 1–5 independent queries of the same tool. Rows run concurrently, keep input order, and fail alone. Rules: [OCTOCODE_WORKFLOWS.md](OCTOCODE_WORKFLOWS.md#make-a-call).

### 3.5 Minimal responses by default
- A result carries the answer and what is needed to continue: `next` pages, open pagination, warnings, partial-coverage signals, and optional `hints`.
- Fields the core output contract classes `verbose` (scan stats, receipts, echoes, diagnostics) appear only with `debug: true`. A contract guard restores any field the output contract requires, so minimizing never breaks a response.
- Details: [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md#minimal-by-default).

### 3.6 Minification
- `minify:"none"` gives exact source (the default for file reads), `"standard"` compacts comments, blank runs, and minified bundles, and `"symbols"` gives an outline of signatures and headings. Strategies follow the format: code, JSON, Markdown, web.
- A minified single-line file is clipped to windows around matches, with `truncated` and the original size reported.
- Positions in a transformed view are not source lines. Cite `none` reads or search hits.

### 3.7 Smart pagination everywhere
- Every list and every large body pages: item pages, content windows inside a file or patch, and whole-response pages. A huge file costs one bounded first page.
- Pages are bound to a snapshot, a SHA-256 fingerprint that proves a cursor still answers its query, so page 2 matches page 1. When the source changes, the page fails with `staleSnapshot` and offers `next.restart`.
- Totals are honest: `hasMore`, `totalPages`, or `countScope:"unknown"`. Provider caps, such as GitHub's 3,000-file PR limit, are reported, never hidden.

### 3.8 Pages, leads, and failure hints
- Follow-up calls use two channels. Both hold executable queries that run unchanged, and the runtime removes follow-ups the current surface cannot run.
  - **`next.*` = pages:** more of the same result, or coverage it still lacks. The response is incomplete without them.
  - **`hints.*` = optional guidance:** `hints.text` holds prose tips; every other entry is a lead, such as read the top match or read the fix PR.
- An empty result carries repair tips in `hints.text`. It is not an absence claim until scope, spelling, ref, and index limits are checked.
- Errors are typed (`invalidInput`, `notFound`, `authentication`, `rateLimited`, …) and carry `retryable: true` only when a retry can help. Each class has its own CLI exit code; see the [CLI guide](../packages/octocode/docs/OCTOCODE_CLI.md#exit-codes).
- GitHub tools follow a renamed repository to its canonical name and add a warning that names it.
- Channels and fields: [TOOL_DATA_CONTRACT.md](TOOL_DATA_CONTRACT.md#executable-continuations).

### 3.9 Precision layers: AST and LSP
- **AST (tree-sitter):** structural patterns (`$X.unwrap()`, `new Error($MSG)`) and YAML rules match code shape, never comments or strings. Symbols give exact declaration inventories. `astRewrite` applies a codemod only after a preview, and only if the files are unchanged since then. Languages: [language and feature reference](../packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md).
- **LSP:** real language servers (tsserver, rust-analyzer, pyright, clangd, …) resolve definitions, references, call hierarchy, hover, and diagnostics from 1-based positions taken from a real anchor. A missing server gives a `serverUnavailable` error and a `hints.textSearch` lead.
- **Ranking:** hits in a file rank declaration > deciding statement (assignment, `if`, `return`, `throw`) > code > comment. For an exact identifier, the file that declares it ranks first.

### 3.10 clasify: semantic judgment before reading
- `clasify` (optional; needs a classification API key) judges unread candidates. **Scout** rates each item of a list result, **Locate** finds the line window that answers a question, and **Judge** answers typed questions about supplied state, including `sufficient` (does this snippet already answer?).
- It returns verdicts and windows, never file bodies, plus the exact `hints.read` for what is worth opening. The provider gets the evidence, the questions, and any brief, never tokens, cursors, or snapshots.
- It pays when the target is described, not named: Scout over a 54-file search returned the answer first for 1.9–11.8 KB against 58 KB of unranked hits, and Locate in a 3 MB file read 16 KB. It costs more when the name is known (1.8 KB by search against 16 KB), for small files and short lists, and when it judges search snippets instead of files (scores stay flat, 0.16–0.38). Scores never prove absence. Read to verify scores from 0.36 to 0.69: its errors fell there. Measurements: [OCTOCODE_CLASIFY.md](OCTOCODE_CLASIFY.md#at-a-glance).
- Without a key, it leaves the tool list and every `hints.*`.
- Modes, cache, and limits: [OCTOCODE_CLASIFY.md](OCTOCODE_CLASIFY.md).

### 3.11 Security: sanitize input and output
- **Input:** the contract is validated before execution; unknown fields are rejected and nothing is silently clamped. Local paths stay inside the sandbox, and sensitive files are refused even inside allowed roots.
- **Output:** the native secret scanner reads every byte bound for the model, including PEM blocks split across page windows, and replaces values with typed markers. Searches never match inside a secret.
- **Egress:** `clasify` is the only tool that sends content to a third party, and its payload excludes credentials and cursors. Details: [SECURITY.md](SECURITY.md).

### 3.12 Local caching and GitHub rate limits
- GitHub file bodies and listings are cached in memory and, with persistent storage, on a disk cache that the CLI and MCP share. Reads at a commit SHA never need revalidation.
- Rate limits are respected: `retryAfterSeconds` and `resetEpochSeconds` are reported, and a local circuit breaker refuses calls ("request not sent") until the window resets. Details: [CONFIGURATION.md](CONFIGURATION.md#cache-storage-and-lifecycle).

### 3.13 The Rust layer: local efficiency
- One native engine runs text search (an in-process regex walker), parsing and structural matching (tree-sitter), minification, redaction, the LSP client pool, the dependency graph, and the GitHub and registry clients. There is nothing extra to install.
- MCP loads it in process through napi, so calls are warm. The CLI is a native binary.
- Concurrent rows share one walk-thread budget, with parse size limits and deadlines on rewrites.

## 4. Strengths and limits

Earlier scripted tool-level comparisons measured single tool calls, not agent outcomes. Agent-against-agent results, including where Octocode loses, are in the [benchmark results](../packages/octocode-benchmark/results/SUMMARY.md).

| Where Octocode wins | Evidence |
|---|---|
| Large PRs | 9.6k against 45k characters over 4 PRs (37–656 files), all answers correct. `gh pr view --json files` stops at 100 files and `gh pr diff` fails above 300; Octocode filters a 656-file PR in one call and flags files sent without a patch. |
| Honest completeness | It reports GitHub's 300-file compare cap (true count 339) and follows renamed repositories where `gh` returns 0. |
| Reproducible evidence | Reads return `commitSha`, pages stay pinned to it, and PRs carry `sourceSha` and `mergeCommitSha`. |
| Precision | LSP references are exact (`rg -w`: 0.26–0.62). Symbols were exact on 6 of 6 files (regexes and ctags: 0.81–0.98). Codemods matched ast-grep on 5 of 5; sed was wrong on 4 of 5. |
| Hostile input | 0 of 5 fake secrets leaked (rg: 3 of 3). A 3.2 MB file costs 18k characters, and a 2 MB minified line costs 710. |
| `clasify` on how and where questions | Whole-file locate: 10 of 10 (rg: 9 of 10), with 43% fewer files opened. |
| Local parity with rg | Same accuracy (14 of 15), with about 8% more characters for the citable line gutter. |

| Where plain tools win | Why |
|---|---|
| Per-call latency | rg answers in 3–25 ms; the CLI starts in about 200 ms per call. MCP is warm, but an LSP cold start takes seconds. |
| Whole-list GitHub reads | A full compare, a deep tree, or a long thread takes several paged calls; one `gh api --jq` pipeline takes one. |
| Tiny answers | Bare `gh` or rg output is a few hundred characters shorter than Octocode's evidence (SHA, lines, pagination). |
| Agent outcomes | In the agent benchmark, `rg` + `gh` still beat Octocode on quality and cost. |

The code that implements each part: [DEVELOPMENT.md](../skills-dev/octocode-dev/docs/DEVELOPMENT.md#concept-to-code).
