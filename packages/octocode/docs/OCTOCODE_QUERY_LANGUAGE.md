# Octocode Query Language (OQL)

OQL is the JSON language behind `npx octocode search`. It is a typed routing
layer over Octocode's primitives — ripgrep text/regex/AST, native graph facts,
content reads, LSP semantics, GitHub, npm, history, binary inspection, and clone.
The schema name is always `"oql"`.

**The critical rule:** OQL returns candidates, proof, and executable next steps.
Never turn a candidate result into a deletion or absence claim until the evidence
says the answer is ready.

## What to Read

Each document is self-contained. Pick the depth you need:

| I need to… | Read |
|---|---|
| Run a query right now | This file: [Cheatsheet](#cheatsheet) → [Decision Tree](#target-selection-decision-tree) → [Common Recipes](#common-recipes) |
| Write queries confidently | This file + [OQL Language Reference](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OQL_LANGUAGE_REFERENCE.md) |
| Interpret results and evidence | [OQL Results and Evidence](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OQL_RESULTS_AND_EVIDENCE.md) |
| Implement or debug a backend transformer | [OQL Internals](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OQL_INTERNALS.md) (contributor-only) |

The live executable contract always wins over prose:

```bash
npx octocode search --scheme          # full OQL schema
npx octocode search --scheme --compact  # compact agent guide
```

---

## How OQL Works

```text
your query (JSON, or CLI shorthand)
  │
  ▼  1. NORMALIZE   sugar → strict canonical OQL; infer target when unambiguous
  ▼  2. PLAN/ROUTE  per-predicate: PUSHDOWN · RESIDUAL · ROUTE · UNSUPPORTED
  ▼  3. TRANSFORM   canonical fields → ONE backing tool (ghSearchCode, localSearchCode,
  │                  lspGetSemantics, npmSearch, localBinaryInspect, …)
  ▼  4. EXECUTE     the backing tool runs (same tool the raw `tools` CLI calls)
  ▼  5. MAP BACK    provider output → stable OQL rows + pagination + diagnostics +
  │                  evidence (proof/partial/candidate/unsupported) + runnable next.*
  ▼
result envelope  ── read evidence, then follow next.* (don't invent follow-ups)
```

The transformer is the only place provider vocabulary lives. Adding a provider
means writing a transformer, not changing the OQL language.

---

## Cheatsheet

One row per target. Use `--query '<json>'` when a lane needs fields that
shorthand cannot express. Note: a bare `packages/foo` path is read as a GitHub
`owner/repo` — prefix local paths with `./`.

| Target | Purpose | Shorthand CLI | Use when |
|---|---|---|---|
| `code` | Text / regex / AST matches | `npx octocode search "runCLI" ./src --lang ts` | Find where a string, pattern, or AST shape appears |
| `content` | Read a file / range / outline | `npx octocode search ./src/index.ts --content-view symbols` | You know the file and want to read it (not search) |
| `structure` | Browse a directory or repo tree | `npx octocode search ./src --tree --depth 2` | Orient before searching; see what exists |
| `files` | Discover files by path/name/ext/size | `npx octocode search "x" ./src --search path --ext ts` | List files matching path/metadata |
| `semantics` | LSP: defs, refs, callers, symbols, hover | `npx octocode search ./src/index.ts --op references --symbol runCLI --line 42` | Prove symbol identity/reachability (run `--op documentSymbols` first for line anchors) |
| `repositories` | GitHub repo discovery | `npx octocode search "mcp server" --target repositories --lang TypeScript --stars ">100"` | Find repos by topic/language/stars |
| `packages` | npm package discovery | `npx octocode search zod --target packages` | Resolve a package + its source repo |
| `pullRequests` | PR search / deep read | `npx octocode search vercel/next.js#1 --target pullRequests --comments --patches` | Inspect a PR's discussion, files, patches |
| `commits` | Commit history + optional diffs | `npx octocode search vercel/next.js/packages/next/src --target commits --since 2024-01-01T00:00:00Z` | "What changed here / when / by whom" |
| `artifacts` | Binary / archive / strings | `npx octocode search dist/server.node --target artifacts --inspect` | Inspect/list/extract/strings a binary or archive |
| `diff` | PR patch OR two-file/two-ref diff | `npx octocode search src/a.ts src/b.ts --target diff` | Compare two files/refs, or read a PR patch |
| `research` | Dead-code / reachability candidates | `--query '{"target":"research","from":{"kind":"local","path":"."},"params":{"intent":"reachability","facets":["symbols","files"]}}'` | "What looks dead, why, what keeps it alive?" Always candidate-grade |
| `graph` | Retained-by chains + bounded LSP proof | `--query '{"target":"graph","from":{"kind":"local","path":"."},"params":{"intent":"reachability","proof":"lsp","proofLimit":5}}'` | Upgrade research candidates with proof |
| `materialize` | Clone/cache a bounded GitHub subtree | `npx octocode clone vercel/next.js/packages/next/src` | Make remote code local for AST/LSP/negation proof |

```bash
npx octocode search --scheme                     # live executable contract
npx octocode search --query '<json>' --json --compact
npx octocode search --explain --query '<json>' --json --compact
```

---

## Target-Selection Decision Tree

Pick the source first, then the answer family, then the matching input.

```text
STEP 1 — pick the SOURCE (where can this be answered?)
  local disk .................. from:{kind:"local", path}          shorthand: ./path
  a GitHub repo ............... from:{kind:"github", repo}          shorthand: owner/repo
  npm registry ............... from:{kind:"npm"}                    shorthand: --target packages
  an already-cloned checkout .. from:{kind:"materialized", localPath}

STEP 2 — pick the TARGET (what answer family?)
  Do you want to MATCH something or READ something?

  MATCH (search):
    code text / regex / AST .......................... target:code       (+ where)
    files by name / ext / size / (not-)containing .... target:files      (+ where)
    repos by topic/stars ............................. target:repositories (+ params)
    npm packages ..................................... target:packages   (+ params)
    PRs / commits .................................... target:pullRequests | commits (+ params)
    binary / archive contents ........................ target:artifacts  (+ params)

  READ (you already know the file/tree/refs):
    a file / range / symbol outline .................. target:content    (+ fetch.content)
    a directory or repo tree ......................... target:structure  (+ fetch.tree)
    a diff between two refs/files or a PR patch ...... target:diff       (+ params)

  PROVE (symbol identity / reachability / dead code):
    "where is X referenced / defined / called?" ...... target:semantics  (+ params.type)
    "what looks dead and why?" ....................... target:research   (candidate-first)
    "what retains it / is the keeper dead?" .......... target:graph      (+ params.proof:"lsp")

  BRIDGE (GitHub returned 0 / need AST/LSP/negation on remote code):
                                                       target:materialize (then re-run local)

STEP 3 — supply the matching input
  target:code|files  → where  (text|regex|structural|field|all/any/not)
  target:content     → fetch.content   (NEVER where)
  target:structure   → fetch.tree       (NEVER where)
  everything else    → params           (target-specific knobs)

STEP 4 — bound + trim
  scope (path/lang/include/exclude/depth) · view (discovery|paginated|detailed)
  · select · limit/page/itemsPerPage · controls

STEP 5 — read the answer
  evidence.answerReady · evidence.complete · evidence.kind · diagnostics
  · follow next.* continuations (do NOT invent follow-up queries)
```

**Key rules:**
- `target:code` REQUIRES a `where` predicate; omitting it is not "search everything."
- `content`/`structure` REJECT `where` — use `fetch` instead.
- GitHub zero rows (`providerUnindexed`) is NOT absence — verify path with `--tree`, then materialize/clone.
- `research`/`graph` are ALWAYS `evidence:"candidate"` / `answerReady:false` — that is normal, not a failure.

---

## Common Recipes

### Orient in an unknown codebase

```bash
npx octocode search ./src --tree --depth 2                  # see the shape
npx octocode search ./src/index.ts --content-view symbols   # outline a file
```

### Find a function/string (local, then read exact)

```bash
npx octocode search "runCLI" ./src --lang ts --view discovery        # locate (paths only)
npx octocode search ./src/cli/index.ts --op documentSymbols          # get line anchors
npx octocode search ./src/cli/index.ts --match-string "runCLI" --content-view exact   # read exact
```

### Enumerate exports with regex

```bash
npx octocode search --query '{
  "target":"code",
  "from":{"kind":"local","path":"./src"},
  "where":{"kind":"regex","value":"^export (function|const|type|interface) [A-Za-z0-9_]+","multiline":true},
  "select":["path","line","snippet","next.semantic"]
}'
```

### Structural AST search

```bash
npx octocode search --pattern 'function $N($$$ARGS) { $$$BODY }' ./src --lang ts
# For a symbol by name, prefer a rule over a bare pattern:
npx octocode search --rule '{"kind":"function_declaration","has":{"pattern":"runCLI"}}' ./src --lang ts
```

> 0 matches + no parse error = your pattern shape does not match the real node. A
> function WITH a return type only matches a pattern that also has `: $RET`. Fall
> back to a rule, `--op documentSymbols`, or a regex inventory.

### Find files (by extension; or files NOT containing text)

```bash
npx octocode search "x" ./src --search path --ext ts          # files by ext
npx octocode search --query '{
  "target":"files",
  "from":{"kind":"local","path":"./src"},
  "where":{"kind":"all","of":[
    {"kind":"field","field":"extension","op":"=","value":"ts"},
    {"kind":"not","predicate":{"kind":"text","value":"MCP_REGISTRY"}}]}}'
```

### Prove where a symbol is used (deletion safety)

```bash
npx octocode search ./src/index.ts --op documentSymbols                          # 1. line anchors
npx octocode search ./src/index.ts --op references --symbol runCLI --line 42     # 2. refs
```

### Search GitHub, recover from a zero result

```bash
npx octocode search "use server" vercel/next.js --lang ts      # provider code search
npx octocode search vercel/next.js/packages/next/src --tree    # verify the path exists
npx octocode search useState packages/next/src --repo vercel/next.js --materialize required
```

### Inspect an npm package, then its source

```bash
npx octocode search zod --target packages
# follow the source-repo continuation into GitHub or materialize
```

### Read a PR deeply / diff two refs

```bash
npx octocode search vercel/next.js#1 --target pullRequests --deep
npx octocode search src/a.ts src/b.ts --target diff
```

### Inspect a binary / archive

```bash
npx octocode search dist/server.node --target artifacts --inspect
npx octocode search app.zip --target artifacts --list
npx octocode search dist/app.bin --target artifacts --strings --min-length 6
```

### Dead-code triage (research → graph proof)

```bash
# Phase 1: summary + first candidate packet
npx octocode search --query '{
  "target":"research",
  "from":{"kind":"local","path":"."},
  "params":{"intent":"reachability","facets":["symbols","files","relations"],"mode":"analyze"},
  "itemsPerPage":1
}'
# Phase 2: follow next.page for packets, then each packet's next.graph (proof:"lsp") for LSP proof.
# Never claim "safe to delete" while evidence.kind=="candidate" or answerReady==false.
```

### Preview routing before running

```bash
npx octocode search --explain --query '{
  "target":"code",
  "from":{"kind":"local","path":"./src"},
  "where":{"kind":"text","value":"term"}
}'
```

---

## Agent Rules and Checklist

**Rules:**

- Choose one `target` first. Never mix `where` with `content`/`structure`.
- Prefer `view:"discovery"` for orientation; use `select` aggressively.
- Use `--explain` before claiming absence, dead code, or safe deletion.
- Treat provider zero results as absence only when `--explain` proves the provider evaluated the exact predicate over the needed universe.
- Treat `research`/`graph` output as candidate evidence until proof continuations are followed.
- Follow `next.*` continuations instead of inventing paths, line ranges, pages, or symbol anchors.
- Read `diagnostics` before answering.
- Cite file paths and lines from proof-grade sources whenever possible.

**One-screen checklist before answering:**

```text
target chosen?
from and scope bounded?
where used only for code/files?
params used for target operation?
fetch used only for reads?
explain checked when proof matters?
diagnostics clean or explicitly reported?
evidence.answerReady true for final claims?
next.* followed for missing proof?
safe-deletion claims backed by LSP/file/package proof?
```

---

## Further Reading

- [OQL Language Reference](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OQL_LANGUAGE_REFERENCE.md) — full spec: query anatomy, all targets, all predicate kinds, params by target, materialization, views/controls/defaults, batches, normalization/explain
- [OQL Results and Evidence](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OQL_RESULTS_AND_EVIDENCE.md) — result envelope, evidence tiers, diagnostics table, continuations, research/graph flows, safe deletion rules
- [OQL Internals](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OQL_INTERNALS.md) — transformer architecture, transformer contract, language selector logic, transformer inventory (contributor-only)
- [CLI Reference](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OCTOCODE_CLI.md) — all commands, flags, and workflows
