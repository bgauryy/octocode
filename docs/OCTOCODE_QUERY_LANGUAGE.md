# Octocode Query Language

OQL is the JSON language behind `npx octocode search`: one typed routing layer over
Octocode's existing primitives — ripgrep code/file search, structural AST,
native graph facts, content reads, LSP semantics, and GitHub/npm/history/binary/
diff/clone runners. It gives humans and agents one consistent way to ask bounded
research questions over code, files, content, symbols, repos, packages, history,
artifacts, diffs, materialized checkouts, and relationship graphs. The schema
name is always `"oql"`.

The important rule: OQL returns candidates, proof, and executable next steps. Do
not turn a candidate into a deletion decision until the evidence says the answer
is ready.

## How OQL Works (one pass)

You never call a provider directly. You write **intent** in general terms —
where to look, what kind of answer you want, which filters apply, and what to
read — and OQL runs it through a fixed pipeline:

```text
your query (JSON, or CLI shorthand)
  │
  ▼  1. NORMALIZE   sugar → strict canonical OQL; infer target when unambiguous;
  │                  reject unknown/ambiguous fields (see Normalization)
  ▼  2. PLAN/ROUTE  decide per-predicate: PUSHDOWN · RESIDUAL · ROUTE · UNSUPPORTED
  │                  (this is what --explain shows you)
  ▼  3. TRANSFORM   the transformer for (target, source) lowers canonical fields
  │                  onto ONE backing tool — ghSearchCode, localSearchCode,
  │                  lspGetSemantics, npmSearch, localBinaryInspect, … — and marks
  │                  each field exact / approximate / lossy
  ▼  4. EXECUTE     the backing tool runs (the same tool the raw `tools` CLI calls)
  ▼  5. MAP BACK    provider output → stable OQL rows + pagination + diagnostics +
  │                  evidence (proof/partial/candidate/unsupported) + runnable next.*
  ▼
result envelope  ── read evidence, then follow next.* (don't invent follow-ups)
```

The **transformer** is the only place provider vocabulary lives. It is the
compliance boundary that keeps the public OQL shape stable while GitHub, npm,
local ripgrep/AST, LSP, and binary inspection keep their own internal APIs — so
the language never changes when a backend does. Adding a provider means writing a
transformer, not changing OQL. (Full contract: the
[Transformer Architecture appendix](#appendix-transformer-architecture-contributor-only--internal).)

## How To Read This Doc (incremental)

Pick your depth — each tier stands on its own:

| You want to… | Read | Time |
|---|---|---|
| Run a query right now | [Cheatsheet](#cheatsheet) → [Quick Start](#quick-start-60-seconds) → [Decision Tree](#target-selection-decision-tree) → [Common Recipes](#common-recipes) | ~5 min |
| Write queries confidently | + [Query Anatomy](#query-anatomy) · [Targets](#targets) · [Predicates](#predicates) · [Fetch](#fetch-and-content-views) · [Params By Target](#params-by-target) | ~15 min |
| Trust/automate the results | + [Result Envelope, Evidence, Diagnostics](#result-envelope-evidence-and-diagnostics) · [Continuations](#continuations) · [Research, Graph, Safe Deletion](#research-graph-and-safe-deletion) | ~15 min |
| Implement or debug a backend | + [Transformer Architecture appendix](#appendix-transformer-architecture-contributor-only--internal) | contributor |

The executable contract always wins over prose: `npx octocode search --scheme`. If
this doc and `--scheme` ever disagree, `--scheme` is correct — open an issue.

## Table of Contents

- [How OQL Works (one pass)](#how-oql-works-one-pass)
- [How To Read This Doc (incremental)](#how-to-read-this-doc-incremental)
- [Cheatsheet](#cheatsheet)
- [Quick Start (60 seconds)](#quick-start-60-seconds)
- [Target-Selection Decision Tree](#target-selection-decision-tree)
- [Common Recipes](#common-recipes)
- [Query Anatomy](#query-anatomy)
- [Targets](#targets)
- [Sources](#sources)
- [Scope](#scope)
- [Predicates](#predicates)
- [Fetch And Content Views](#fetch-and-content-views)
- [Params By Target](#params-by-target)
- [Materialization And GitHub Index Misses](#materialization-and-github-index-misses)
- [Views, Select, Controls, And Defaults](#views-select-controls-and-defaults)
- [Batches](#batches)
- [Normalization And Explain](#normalization-and-explain)
- [Result Envelope, Evidence, And Diagnostics](#result-envelope-evidence-and-diagnostics)
- [Continuations](#continuations)
- [Research, Graph, And Safe Deletion](#research-graph-and-safe-deletion)
- [Agent Rules And Checklist](#agent-rules-and-checklist)
- [Appendix: Transformer Architecture (contributor-only / internal)](#appendix-transformer-architecture-contributor-only--internal)

## Cheatsheet

One row per target. Shorthand is verified live; use `--query '<json>'` when a
lane needs fields shorthand cannot express. Run `npx octocode search --scheme` for
the full contract. Note: a bare `packages/foo` path is read as a GitHub
`owner/repo` — prefix local paths with `./` (or pass `--source local`).

| Target | Purpose | Copy-paste CLI | Use when |
|---|---|---|---|
| `code` | Text / regex / AST matches | `npx octocode search "runCLI" ./src --lang ts` | Find where a string, pattern, or AST shape appears. |
| `content` | Read a file / range / symbol outline | `npx octocode search ./src/index.ts --content-view symbols` | You know the file and want to read it (not search). |
| `structure` | Browse a directory or repo tree | `npx octocode search ./src --tree --depth 2` | Orient before searching; see what exists. |
| `files` | Discover files by path/name/ext/size | `npx octocode search "x" ./src --search path --ext ts` | List files matching path/metadata, or files (not) containing text. |
| `semantics` | LSP: defs, refs, callers, symbols, hover | `npx octocode search ./src/index.ts --op references --symbol runCLI --line 42` | Prove symbol identity/reachability (run `--op documentSymbols` first for line anchors). |
| `repositories` | GitHub repo discovery | `npx octocode search "mcp server" --target repositories --lang TypeScript --stars ">100"` | Find repos by topic/language/stars. |
| `packages` | npm package discovery | `npx octocode search zod --target packages` | Resolve a package + its source repo. |
| `pullRequests` | PR search / deep read | `npx octocode search facebook/react#1 --target pullRequests --comments --patches` | Inspect a PR's discussion, files, patches. |
| `commits` | Commit history (+ optional diffs) | `npx octocode search facebook/react/packages/react/src --target commits --since 2024-01-01T00:00:00Z` | "What changed here / when / by whom." |
| `artifacts` | Binary / archive / strings inspection | `npx octocode search dist/server.node --target artifacts --inspect` | Inspect/list/extract/strings a binary or archive. |
| `diff` | PR patch OR two-ref/two-file diff | `npx octocode search src/a.ts src/b.ts --target diff` | Compare two files/refs, or read a PR patch. (Two-ref content diff now produces a real line diff.) |
| `research` | Candidate dead-code / reachability packets | `npx octocode search --query '{"target":"research","from":{"kind":"local","path":"./src"},"params":{"intent":"reachability","facets":["symbols","files"]},"itemsPerPage":1}'` | "What looks dead, why, what keeps it alive?" Always candidate-grade. |
| `graph` | Retained-by chains + bounded LSP proof | `npx octocode search --query '{"target":"graph","from":{"kind":"local","path":"./src"},"params":{"intent":"reachability","facets":["symbols"],"proof":"lsp","proofLimit":5}}'` | "What retains this? Is the keeper itself dead?" Upgrade research candidates. |
| `materialize` | Clone/cache a bounded GitHub subtree | `npx octocode clone facebook/react/packages/react/src` (or `--target materialize`) | Make remote code behave like local for AST/LSP/negation proof. |

The live CLI schema is the executable contract:

```bash
npx octocode search --scheme
npx octocode search --query '<json>' --json --compact
npx octocode search --explain --query '<json>' --json --compact
```

Inside this monorepo, the local built CLI is
`npx octocode search --scheme`. MCP exposes the same
schema through the thin `oqlSearch` tool; the CLI and MCP tool import the shared
OQL schema rather than duplicating the shape.

Shorthand covers the common flows shown in the cheatsheet. Use `--query` JSON
when a lane needs fields shorthand cannot express (boolean predicates,
`includeDeclaration:false`, research/graph params, field-set negation, etc.).

## Quick Start (60 seconds)

Read this document top to bottom the first time. After that, follow this path:

1. Choose a `target`.
2. Set `from` and an optional `scope`.
3. Use `where` only for code/file predicates.
4. Use `params` for target-specific operations.
5. Use `fetch` for content and tree reads.
6. Keep output small with `view`, `select`, and `controls`.
7. Read `diagnostics`, `provenance`, `evidence`, and `next`.
8. Follow `next.*` continuations instead of inventing follow-up queries.

Minimal query:

```json
{
  "schema": "oql",
  "target": "code",
  "from": { "kind": "local", "path": "./src" },
  "where": { "kind": "text", "value": "runCLI" },
  "view": "paginated",
  "select": ["path", "line", "snippet", "next.fetch"]
}
```

`schema` may be omitted by callers; normalization inserts `"oql"`.

## Target-Selection Decision Tree

This single tree replaces the older lane/selection/language tables. Pick the
source first, then the answer family, then the matching input.

```text
STEP 1 — pick the SOURCE (where can this be answered?)
  local disk .................. from:{kind:"local", path}     shorthand: ./path
  a GitHub repo ............... from:{kind:"github", repo}     shorthand: owner/repo
  npm registry ............... from:{kind:"npm"}               shorthand: --target packages
  an already-cloned checkout .. from:{kind:"materialized", localPath}

STEP 2 — pick the TARGET (what answer family?)
  Do you want to MATCH something or READ something?

  MATCH (search):
    code text / regex / AST .......................... target:code   (+ where)
    files by name / ext / size / (not-)containing .... target:files  (+ where)
    repos by topic/stars ............................. target:repositories (+ params)
    npm packages ..................................... target:packages (+ params)
    PRs / commits .................................... target:pullRequests | commits (+ params)
    binary / archive contents ........................ target:artifacts (+ params)

  READ (you already know the file/tree/refs):
    a file / range / symbol outline .................. target:content    (+ fetch.content)
    a directory or repo tree ......................... target:structure  (+ fetch.tree)
    a diff between two refs/files or a PR patch ...... target:diff       (+ params)

  PROVE (symbol identity / reachability / dead code):
    "where is X referenced / defined / called?" ...... target:semantics  (+ params.type)
    "what looks dead and why?" ....................... target:research   (candidate-first)
    "what retains it / is the keeper dead?" .......... target:graph      (+ params.proof:"lsp")

  BRIDGE:
    "GitHub returned 0 / I need AST/LSP/negation on remote code"
                                                       target:materialize  (then re-run local)

STEP 3 — supply the matching input
  target:code|files  -> where  (text|regex|structural|field|all/any/not)
  target:content     -> fetch.content   (NEVER where)
  target:structure   -> fetch.tree       (NEVER where)
  everything else    -> params           (target-specific knobs)

STEP 4 — bound + trim
  scope (path/lang/include/exclude/depth) · view (discovery|paginated|detailed)
  · select · limit/page/itemsPerPage · controls

STEP 5 — read the answer
  evidence.answerReady · evidence.complete · evidence.kind · diagnostics
  · follow next.* continuations (do NOT invent follow-up queries)
```

Key rule callouts:

- `target:code` REQUIRES a `where`; omitting it is not "search everything."
- `content`/`structure` REJECT `where` — use `fetch`.
- GitHub zero rows = `providerUnindexed`, NOT absence. Verify the path with
  `--tree`, then `--materialize required`, `npx octocode clone`, or
  `npx octocode cache fetch`.
- `research`/`graph` are ALWAYS `evidence:"candidate"` / `answerReady:false` —
  that is normal, not a failure.

## Common Recipes

Each recipe gives the goal, the one-line shorthand, and canonical JSON only when
shorthand cannot express it. Verified live 2026-06-24. Local paths are
`./`-prefixed so the CLI does not read them as `owner/repo`.

**Orient in an unknown codebase**

```bash
npx octocode search ./src --tree --depth 2                  # see the shape
npx octocode search ./src/index.ts --content-view symbols   # outline a file
```

**Find a function/string (local, then read exact)**

```bash
npx octocode search "runCLI" ./src --lang ts --view discovery        # locate (paths only)
npx octocode search ./src/cli/index.ts --op documentSymbols          # get line anchors
npx octocode search ./src/cli/index.ts --match-string "runCLI" --content-view exact   # read exact
```

**Enumerate exports with regex**

```bash
npx octocode search --query '{"target":"code","from":{"kind":"local","path":"./src"},
  "where":{"kind":"regex","value":"^export (function|const|type|interface) [A-Za-z0-9_]+","multiline":true},
  "select":["path","line","snippet","next.semantic"]}'
```

**Structural AST search (pattern must match the COMPLETE node)**

```bash
npx octocode search --pattern 'function $N($$$ARGS) { $$$BODY }' ./src --lang ts
# To find a symbol by name robustly, prefer a rule over a bare pattern:
npx octocode search --rule '{"kind":"function_declaration","has":{"pattern":"runCLI"}}' ./src --lang ts
```

> 0 matches + no parse error = your pattern shape does not match the real node. A
> function WITH a return type only matches a pattern that also has `: $RET`. Fall
> back to a rule, `--op documentSymbols`, or a regex inventory.

**Find files (by extension; or files NOT containing text)**

```bash
npx octocode search "x" ./src --search path --ext ts          # files by ext
npx octocode search --query '{"target":"files","from":{"kind":"local","path":"./src"},
  "where":{"kind":"all","of":[
    {"kind":"field","field":"extension","op":"=","value":"ts"},
    {"kind":"not","predicate":{"kind":"text","value":"MCP_REGISTRY"}}]}}'   # negation needs local universe
```

**Prove where a symbol is used (deletion safety)**

```bash
npx octocode search ./src/index.ts --op documentSymbols                          # 1. line anchors
npx octocode search ./src/index.ts --op references --symbol runCLI --line 42     # 2. refs (set includeDeclaration:false via --query)
```

**Search GitHub, recover from a zero result (providerUnindexed)**

```bash
npx octocode search "createServer" facebook/react              # provider code search
npx octocode search facebook/react/packages/react/src --tree   # verify the path exists
npx octocode search useState packages/react/src --repo facebook/react --materialize required   # bounded local proof
```

**Inspect an npm package, then its source**

```bash
npx octocode search zod --target packages
# then follow the source-repo continuation into GitHub or materialize
```

**Read a PR deeply / diff two refs**

```bash
npx octocode search facebook/react#1 --target pullRequests --deep
npx octocode search src/a.ts src/b.ts --target diff
```

**Inspect a binary / archive**

```bash
npx octocode search dist/server.node --target artifacts --inspect
npx octocode search app.zip --target artifacts --list
npx octocode search dist/app.bin --target artifacts --strings --min-length 6
```

**Dead-code triage (two-phase research, then graph proof)**

```bash
# 1. summary page (counts) + first candidate packet
npx octocode search --query '{"target":"research","from":{"kind":"local","path":"."},
  "params":{"intent":"reachability","facets":["symbols","files","relations"],"mode":"analyze"},"itemsPerPage":1}'
# 2. follow next.page for packets, then the row's pre-filled next.graph (proof:"lsp") for bounded LSP proof.
# Never claim "safe to delete" while evidence.kind=="candidate" or answerReady==false.
```

**See routing before running (proof-sensitive queries)**

```bash
npx octocode search --explain --query '{"target":"code","from":{"kind":"local","path":"./src"},"where":{"kind":"text","value":"term"}}'
```

## Query Anatomy

Every normal query has the same shape:

```ts
interface OqlQuery {
  schema?: "oql";
  id?: string;
  target: OqlActiveTarget;
  from?: QuerySource;
  scope?: QueryScope;
  where?: Predicate;
  materialize?: MaterializePolicy | "never" | "auto" | "required";
  fetch?: FetchInstructions;
  params?: Record<string, unknown>;
  select?: string[];
  view?: "discovery" | "paginated" | "detailed";
  controls?: QueryControls;
  limit?: number;
  page?: number;
  itemsPerPage?: number;
  explain?: boolean;
}
```

Field roles:

| Field | Meaning | Use it for |
|---|---|---|
| `target` | Result family | Choose what kind of answer you want. Pick this before any other field. |
| `from` | Corpus or provider | Local path, GitHub scope, materialized path, or npm. Required for corpus-backed targets. |
| `scope` | Bounds inside the corpus | Path, language, include/exclude globs, depth, hidden/no-ignore. Bound every expensive or absence-sensitive query. |
| `where` | Code/file predicate | Text, regex, structural AST, file field, boolean combinations. Used by `code` and `files` only. |
| `params` | Target-specific options | LSP type, repo filters, package query, PR number, artifact mode, research goal, etc. |
| `fetch` | Read instructions | Exact content, compact content, symbol outline, tree settings. Used by `content` and `structure`. |
| `materialize` | Remote-to-local policy | Allow or require bounded GitHub materialization for local proof. |
| `select` | Projection | Return only the fields an agent needs (row, envelope, and `next.*` fields). |
| `view` | Density | Path-only discovery, normal paginated rows, or detailed rows. |
| `controls` | Output/cost controls | Match windows, max matches, budgets, sort, ranking. |
| `limit` | Total cap | Cap the total returned results where the target supports it. |
| `page` | Result page | Top-level page number for windowing/continuations. Follow `next.page`. |
| `itemsPerPage` | Page size | Page the target's primary result domain. For code search this may be matched files; per-file matches use `controls.search.matchPage`. |
| `explain` | Routing visibility | Include normalized query, defaults, plan, backend calls, diagnostics. Use before final proof claims. |

Plain-language mapping:

| General idea | OQL field |
|---|---|
| Where to look | `from` |
| What kind of answer to return | `target` |
| Bounds inside that source | `scope` |
| Match/filter conditions | `where` |
| What to read once a file/tree is known | `fetch` |
| Options specific to one answer type | `params` |
| Response shape, projected fields, and cost limits | `view`, `select`, `controls` |
| Paging the target's primary result domain | `page`, `itemsPerPage` |

OQL is a language, not a parser. Rules of thumb for each field: `target` is what
kind of answer should come back; `from` is which universe can be searched or
read (local/materialized for proof, GitHub/npm for discovery or provider-native
facts); `scope` is which subset matters; `where` is what must match (search and
file-set logic only); `fetch` is what to read; `params` are target-specific
options, not a replacement for `where` or `fetch`; and `evidence` tells you
whether the result is proof, partial, candidate, or unsupported — never upgrade
candidate evidence in prose.

Paging note: `itemsPerPage` means "page size for this target's primary result
domain." For code search, the backing tool may page matched files while each file
can contribute several match rows; noisy per-file matches use
`controls.search.matchPage` / `--match-page`.

## Targets

Active targets:

| Target | Returns | Best for | Main backing tool |
|---|---|---|---|
| `code` | `kind:"code"` rows | Text, regex, and structural AST matches | `localSearchCode` or `ghSearchCode` |
| `content` | `kind:"content"` rows | Exact/compact/symbol file reads | `localGetFileContent` or `ghGetFileContent` |
| `structure` | `kind:"tree"` rows | Directory/repo structure | `localViewStructure` or `ghViewRepoStructure` |
| `files` | `kind:"file"` rows | File discovery and file-set predicates | `localFindFiles` plus `localSearchCode` when needed |
| `semantics` | `recordType:"semantics"` | LSP definitions, references, symbols, calls, hover, workspace symbols, type hierarchy, diagnostics | `lspGetSemantics` |
| `repositories` | `recordType:"repository"` | GitHub repository discovery | `ghSearchRepos` |
| `packages` | `recordType:"package"` | npm package discovery | `npmSearch` |
| `pullRequests` | `recordType:"pullRequest"` | PR search, PR detail, review, patch pages | `ghHistoryResearch` |
| `commits` | `recordType:"commit"` | Commit history and optional diffs | `ghHistoryResearch` |
| `artifacts` | `recordType:"artifact"` | Binary/archive/strings/decompress/extract | `localBinaryInspect` |
| `diff` | `recordType:"diff"` | PR patch diff or two-ref file diff | `ghHistoryResearch` or content diff |
| `research` | `recordType:"research"` | Smart local research packets | OQL research analyzer |
| `graph` | `recordType:"graph"` | Relationship nodes, edges, facts, and missing proof | OQL graph analyzer |
| `materialize` | `recordType:"materialized"` | Clone/cache a bounded GitHub corpus | `ghCloneRepo` and cache lanes |

Reserved targets:

| Target | Status |
|---|---|
| `fixes` | Reserved until OQL can return safe dry-run edits. |
| `dataflow` | Reserved until OQL can return trace-backed flow proof. |

Reserved targets return `unsupportedTarget`. Do not document or use them as
active language features.

## Sources

```ts
type QuerySource =
  | { kind: "local"; path: string }
  | { kind: "github"; repo?: string; owner?: string; ref?: string }
  | { kind: "materialized"; localPath: string; source?: QuerySource }
  | { kind: "npm" };
```

Source rules:

- `local.path` is a file or directory on disk. A local row's `path` is relative
  to `from.path`, but the pre-filled `next.fetch` carries the resolved ABSOLUTE
  path — follow it directly rather than re-joining paths yourself.
- A `local`/`materialized` path that does not exist on disk returns a blocking
  `invalidQuery` diagnostic (`answerReady:false`), NOT an empty proof. A typo'd
  path can therefore never be mistaken for "confirmed absent" — fix the path and
  re-run. (LSP semantics over a directory or missing file likewise return an
  actionable `not_a_file` / `file_not_found` error, never a raw `EISDIR`.)
- `github.repo` is usually `"owner/name"`; `ref` is optional.
- `github.owner` can be used for provider discovery targets.
- `materialized.localPath` is a local checkout returned by `target:"materialize"`
  or clone/cache flows.
- `npm` is for package registry discovery.
- `packages` and `repositories` can run without a local code corpus.
- `content` and `structure` over GitHub need a concrete repository.

## Scope

```ts
interface QueryScope {
  path?: string | string[];
  language?: string | string[];
  include?: string[];     // globs, max 100
  exclude?: string[];     // globs, max 100
  excludeDir?: string[];  // dir globs, max 100
  hidden?: boolean;       // include dotfiles
  noIgnore?: boolean;     // ignore .gitignore
  minDepth?: number;      // 0-64
  maxDepth?: number;      // 0-64
}
```

Use `scope` to bound cost and avoid ambiguous answers. Prefer the smallest
directory, subtree, language set, or include glob that can answer the question.

`scope.language` is canonical OQL intent, not a backend field. It may name an
exact extension (`ts`) or a language family (`typescript`). Backends project it
differently — exact extension where supported, provider language filters, local
extension families, include globs, or LSP server hints. Multiple values are
never silently dropped, and an unknown language selector is never proof of
absence. (See the appendix for the per-backend lowering matrix.)

Examples:

```json
{
  "scope": {
    "path": ["src", "packages/octocode/src"],
    "language": ["ts", "tsx"],
    "excludeDir": ["node_modules", "dist", "coverage"]
  }
}
```

```json
{
  "scope": {
    "include": ["**/*.ts"],
    "exclude": ["**/*.test.ts"],
    "maxDepth": 8
  }
}
```

## Predicates

`where` is a discriminated predicate tree, used by `code` and `files` for
matching (search-index filtering), never for reading.

`target:"code"` requires a `where` predicate. OQL does not treat omitted `where`
as "search everything." `target:"content"` and `target:"structure"` reject
`where`; use `fetch` instead. To read a matched file slice, use
`fetch.content.match`. For PR/commit/artifact text filters, use that target's
`params`.

### Text

```json
{ "kind": "text", "value": "runCLI" }
```

| Field | Values |
|---|---|
| `value` | Required string |
| `case` | `smart`, `sensitive`, `insensitive` |
| `wholeWord` | boolean |

### Regex

```json
{
  "kind": "regex",
  "value": "^export (function|const|type|interface) [A-Za-z0-9_]+",
  "multiline": true
}
```

| Field | Values |
|---|---|
| `value` | Required regex string |
| `dialect` | `rust`, `pcre2`, `provider` |
| `case` | `smart`, `sensitive`, `insensitive` |
| `wholeWord` | boolean |
| `multiline` | boolean |
| `dotAll` | boolean |

Defaults: local regex uses the Rust dialect unless a query requests another
dialect the backend can run.

### Structural AST

```json
{
  "kind": "structural",
  "lang": "typescript",
  "pattern": "function $NAME($$$ARGS): $RET { $$$BODY }"
}
```

Or use a JSON rule object:

```json
{
  "kind": "structural",
  "lang": "typescript",
  "rule": { "pattern": "eval($X)" }
}
```

Or use the same YAML rule string accepted by `search --rule`:

```json
{
  "kind": "structural",
  "lang": "typescript",
  "rule": "rule:\n  pattern: \"eval($X)\""
}
```

| Field | Values |
|---|---|
| `lang` | Required language id, such as `typescript`, `javascript`, `python`, `rust` |
| `pattern` | Code-shaped AST pattern |
| `rule` | Relational AST rule, either a JSON object or a grep-compatible YAML rule string |

Use exactly one of `pattern` or `rule`.

Structural patterns must match the COMPLETE AST node, so include the parts the
real node has. A function WITH a return type only matches if the pattern has one
too (`function $N($$$A): $R { $$$B }`); omitting it returns 0. Useful shapes:
`function $N($$$A) { $$$B }` (no-return-type fn), `($$$A) => $$$B` (arrow),
`$F($$$A)` (call), `$O.$M($$$A)` (method). For "find symbol X" the ROBUST form is
a rule, not a pattern: `{ "kind": "function_declaration", "has": { "pattern":
"X" } }`. A `$$$`-only pattern with no literal anchor skips files with no
prefilter hit (low counts); add a literal name or use a regex `where`.

0 matches with a `partialParse`/`partialResult` diagnostic and no parse error
means your pattern shape does not match the real node — treat it as missing
proof, not absence. Fall back to: a narrower rule; `target:"semantics"` with
`documentSymbols`; a regex export inventory; or exact content reads around
candidate lines.

Structural rules support:

```ts
interface StructuralRule {
  pattern?: string;
  kind?: string;
  inside?: StructuralRule;
  has?: StructuralRule;
  not?: StructuralRule;
  all?: StructuralRule[];
  any?: StructuralRule[];
  stopBy?: "end";
}
```

### Field

```json
{ "kind": "field", "field": "extension", "op": "=", "value": "ts" }
```

| Field | Values |
|---|---|
| `field` | `path`, `basename`, `extension`, `size`, `modified`, `accessed`, `entryType`, `empty`, `permissions`, `executable`, `readable`, `writable` |
| `op` | `=`, `!=`, `in`, `exists`, `glob`, `regex`, `>`, `>=`, `<`, `<=`, `within`, `before` |
| `value` | Required except when `op:"exists"` |

Field/op pairings (what each field is for):

| Field | Type | Typical ops | Example value |
|---|---|---|---|
| `path` | string | `=` `glob` `regex` `in` | `"src/index.ts"`, `"src/**/*.ts"` |
| `basename` | string | `=` `glob` `regex` `in` | `"index.ts"` |
| `extension` | string | `=` `!=` `in` | `"ts"`, `["ts","tsx"]` |
| `size` | bytes | `>` `>=` `<` `<=` `=` | `1048576` |
| `modified` / `accessed` | time | `within` `before` `>` `<` | `"7d"`, `"2024-01-01"` |
| `entryType` | enum | `=` | `"f"` (file) or `"d"` (dir) |
| `empty` | flag | `exists` / `=` | `true` |
| `permissions` | octal string | `=` | `"755"` |
| `executable` / `readable` / `writable` | flag | `exists` / `=` | `true` |

Use symbolic ops like `=`; aliases such as `eq` are invalid. There is no
`contains` op — use `op:"glob"` with `value:"*term*"`, or `op:"regex"`.

### Boolean

```json
{
  "kind": "all",
  "of": [
    { "kind": "field", "field": "extension", "op": "=", "value": "ts" },
    { "kind": "not", "predicate": { "kind": "text", "value": "MCP_REGISTRY" } }
  ]
}
```

Forms:

```ts
{ kind: "all", of: Predicate[] }
{ kind: "any", of: Predicate[] }
{ kind: "not", predicate: Predicate }
```

Negation needs a complete universe. Local and materialized sources can prove
negative predicates. Provider search often cannot prove absence unless the query
materializes a bounded corpus.

## Fetch And Content Views

`fetch` reads content or trees. It does not search.

```ts
interface FetchInstructions {
  content?: {
    range?: { startLine?: number; endLine?: number; contextLines?: number };
    match?: { text: string; regex?: boolean; caseSensitive?: boolean };
    contentView?: "exact" | "compact" | "symbols";
    charOffset?: number;
    charLength?: number;
    fullContent?: boolean;
  };
  tree?: {
    maxDepth?: number;        // 0-64
    pattern?: string;         // filter entries by name/glob
    includeSizes?: boolean;
    extensions?: string[];    // keep only these file extensions
    filesOnly?: boolean;
    directoriesOnly?: boolean;
    sortBy?: "name" | "size" | "time" | "extension";
    reverse?: boolean;
  };
}
```

Content views:

| View | Meaning |
|---|---|
| `exact` | Exact source text, best for citations and patches. |
| `compact` | Minified/compact content, default for token efficiency. |
| `symbols` | File outline or symbol skeleton, best for orientation. |

To read the region around a string, anchor with `fetch.content.match` (NOT a
top-level `where`, which is code/files only).

Examples:

```json
{
  "schema": "oql",
  "target": "content",
  "from": { "kind": "local", "path": "./src/index.ts" },
  "fetch": {
    "content": {
      "range": { "startLine": 1, "endLine": 80 },
      "contentView": "exact"
    }
  }
}
```

```json
{
  "schema": "oql",
  "target": "content",
  "from": { "kind": "local", "path": "./src/index.ts" },
  "fetch": {
    "content": {
      "match": { "text": "runCLI" },
      "contentView": "exact"
    }
  }
}
```

If a content result reports `sanitized`, trust the diagnostic. Prefer
`match.text` anchors or LSP `lineHint` anchors over hard-coded line math when
redaction diagnostics are present.

## Params By Target

`params` is for target-specific options. OQL validates common fields early, then
the backing tool remains the exhaustive validator. The tables below mirror
`npx octocode search --scheme`; fields accepted only by an internal pass-through are
not part of the agent contract until the scheme lists them.

### `semantics`

Backs onto `lspGetSemantics`.

```json
{
  "target": "semantics",
  "from": { "kind": "local", "path": "./src/index.ts" },
  "params": {
    "type": "references",
    "symbolName": "runCLI",
    "lineHint": 42,
    "includeDeclaration": false,
    "groupByFile": true,
    "format": "structured"
  }
}
```

| Field | Values |
|---|---|
| `type` | `definition`, `references`, `callers`, `callees`, `callHierarchy`, `hover`, `documentSymbols`, `typeDefinition`, `implementation`, `workspaceSymbol`, `supertypes`, `subtypes`, `diagnostic` |
| `uri` | optional file uri/path override (required for all types except `workspaceSymbol`) |
| `symbolName` | symbol to resolve near `lineHint`; fuzzy search query for `workspaceSymbol` |
| `symbolKind` | filter returned symbol rows after `documentSymbols`/`workspaceSymbol` |
| `lineHint` | 1-based line anchor (required except for `documentSymbols`, `workspaceSymbol`, `diagnostic`) |
| `orderHint` | integer disambiguation hint |
| `includeDeclaration` | boolean |
| `depth` | integer 0-20 |
| `contextLines` | call-flow snippet context for `callers`, `callees`, `callHierarchy` |
| `groupByFile` | boolean |
| `workspaceRoot` | optional workspace root |
| `format` | `structured` or `compact` |

**LSP 3.17 additions:**

| Type | Anchor | Use when |
|---|---|---|
| `workspaceSymbol` | `symbolName` only (no file or position) | Find all symbols matching a name project-wide; useful when you don't know the file |
| `supertypes` | `uri` + `symbolName` + `lineHint` | Walk up an inheritance chain (what does this class/interface extend?) |
| `subtypes` | `uri` + `symbolName` + `lineHint` | Walk down an inheritance chain (what implements this interface?) |
| `diagnostic` | `uri` only | Pull errors/warnings for a file from the language server (LSP pull model) |

`diagnostic` requires a language server that supports the pull-diagnostic
protocol (`textDocument/diagnostic`, LSP 3.17). Servers that only push
diagnostics return an `unsupportedOperation` payload with a migration hint.

Remote GitHub semantics has no provider-only lane. When `from.kind:"github"`,
OQL sparsely materializes the requested `params.uri`/repo and then runs LSP
locally. Normalization reports `materialize:{mode:"required",strategy:"file"}` in
`--explain`, even if the caller omitted materialization.

For deletion/reachability questions, `references` with
`includeDeclaration:false` is the key proof operation.

### `repositories`

Backs onto `ghSearchRepos`.

| Field | Values |
|---|---|
| `keywords` | string array (array even for one term) |
| `topicsToSearch` | string array |
| `language` | string |
| `owner` | string |
| `stars` | string or number range |
| `license` | string |
| `archived` | boolean |
| `sort` | `stars`, `forks`, `help-wanted-issues`, `updated`, `best-match` |
| `limit` | positive integer |
| `page` | positive integer |

The CLI exposes additional repository filters as flags: `--forks`, `--created`,
`--updated`, `--size`, `--visibility`, and `--good-first-issues`. See
`npx octocode search --help` for the full filter set.

### `packages`

Backs onto `npmSearch`.

| Field | Values |
|---|---|
| `packageName` | exact or near package name |
| `keywords` | string array |
| `mode` | `lean` or `full` |
| `page` | positive integer |

Use either `packageName` or `keywords`.

### `pullRequests`

Backs onto `ghHistoryResearch`.

| Field | Values |
|---|---|
| `prNumber` | positive integer |
| `state` | `open`, `closed`, `merged` |
| `author` | string |
| `label` | string or string array |
| `keywordsToSearch` | string array |
| `reviewMode` | backing tool review mode |
| `filePage`, `commentPage`, `commitPage` | positive integers |
| `matchString` | content filter over fetched PR title/body/comments/reviews (per `matchScope`; default `body`) — not a search-index query, so no match yields `zeroMatches` |
| `matchScope` | `body`, `title`, `comments`, `reviews`, `all` |
| `limit`, `page` | pagination |

### `commits`

Backs onto `ghHistoryResearch` with commit mode.

| Field | Values |
|---|---|
| `path` | optional path filter |
| `branch` | optional branch |
| `since`, `until` | date strings accepted by the backing tool |
| `includeDiff` | boolean |
| `matchString` | filters commit messages |
| `filePage`, `itemsPerPage` | changed-file pagination for commit diffs |
| `limit`, `page` | pagination |

### `artifacts`

Backs onto `localBinaryInspect`.

| Field | Values |
|---|---|
| `mode` | `inspect`, `list`, `extract`, `decompress`, `strings`, `unpack` |
| `entryPageNumber` | archive entry page |
| `minLength` | string-scan minimum length, 1-128 |
| `scanOffset` | string scan continuation offset |
| `matchString` | filters text-producing modes (`extract`/`decompress`/`strings`) over the current fetched payload |
| `verbose` | expanded `list` output |

For `strings`, the full scan is always written to `data.localPath`; the inline
`content` is only a small preview (capped well below the global content window)
when you don't ask for an explicit `charOffset`/`charLength` or a `matchString`.
That keeps the default response lean — grep the file for the real work: follow
`next.search` on `data.localPath` for lossless ripgrep paging, and
`next.artifactStrings` (`scanOffset`) to advance to the next binary scan window.
`extract`/`unpack`/`decompress` produce a tree at `data.localPath`
(`next.structure`/`next.files`).

### `diff`

Two lanes are supported.

PR patch lane:

```json
{
  "target": "diff",
  "from": { "kind": "github", "repo": "owner/name" },
  "params": { "prNumber": 123, "files": ["src/index.ts"] }
}
```

Direct file lane:

```json
{
  "target": "diff",
  "from": { "kind": "github", "repo": "owner/name" },
  "params": {
    "baseRef": "main",
    "headRef": "feature",
    "path": "src/index.ts"
  }
}
```

The direct two-ref lane now produces a real line diff (it fetches both refs via
`ghGetFileContent` and computes a local line diff). If neither lane is present,
OQL returns an `invalidQuery` diagnostic with a repair message.

### `research`

The research target returns a smart local research packet. It is the right
starting point for knip-like questions, but it is candidate-first.

```json
{
  "schema": "oql",
  "target": "research",
  "from": { "kind": "local", "path": "." },
  "params": {
    "goal": "what looks dead, why, what keeps it alive, and what proof is missing?",
    "intent": "reachability",
    "facets": ["symbols", "files", "dependencies", "relations"],
    "mode": "analyze",
    "maxFiles": 200
  },
  "view": "paginated",
  "page": 1,
  "itemsPerPage": 25
}
```

| Field | Values |
|---|---|
| `goal` | natural-language research goal |
| `intent` | `general`, `reachability`, `dependencies`, `symbols` |
| `facets` | `symbols`, `files`, `dependencies`, `relations` |
| `mode` | `plan`, `analyze`, `prove` |
| `maxFiles` | positive integer |
| top-level `page` / `itemsPerPage` | page through the generated packet list |

Modes:

| Mode | Meaning |
|---|---|
| `plan` | Return the research flow without scanning files. |
| `analyze` | Return candidate summary, graph packets, and continuations. |
| `prove` | Require explicit `params.intent`; return candidate-grade packets that say which `next.*` proof steps must be followed. |

The research row can include `summary`, `graphCapabilities`,
`nativeGraphSummary`, `graphSummary`, `packetPage`, `packets`, and `caveats`. In
`view:"detailed"`, it can also include raw analyzer arrays: `manifests`, `files`,
`dependencies`, `symbols`, `graphFacts`. Each packet can include `subject`,
`verdict`, `proofStatus`, `why`, `retainedBy`, `missingProof`, `risk`, and
`next`.

The research result row itself can carry `next.graph`. That row-level
continuation is page-aligned and upgrades the current packet page to
`target:"graph"` with bounded LSP proof. Packet-level `next` entries are for
exact reads, semantic checks, or text/dynamic follow-up on the individual
candidate.

Research packets use native AST facts where available, capability-driven not
JS/TS-hardcoded: JS/TS uses OXC graph facts, other supported source languages use
tree-sitter graph inventory. `graphCapabilities` tells agents which extensions
can emit graph facts, which languages were seen, and whether any source files
missed graph extraction. AST graph facts prove syntax inventory (declarations,
imports, exports, calls, containment, source locations); cross-file references
and callers stay candidate evidence until LSP proof is attached.

If `pagination.hasMore:true`, follow top-level `next.page` before calling the
packet set complete.

### `graph`

The graph target returns an agent-readable relationship graph built from the same
research packet universe as `target:"research"`, enriched by native AST graph
facts where available. Use it when the question is about relationships rather than
the full packet list: what retains this symbol/file; what this subject retains;
which candidate-dead nodes are only kept alive by other candidate-dead nodes;
which proof is missing for the current page; what continuations to follow next.

```json
{
  "schema": "oql",
  "target": "graph",
  "from": { "kind": "local", "path": "." },
  "params": {
    "goal": "show retained-by chains for dead-looking exports",
    "intent": "reachability",
    "facets": ["symbols", "files", "dependencies", "relations"],
    "verdict": "transitive-dead",
    "relation": "references",
    "direction": "incoming",
    "proof": "lsp",
    "proofLimit": 5,
    "includePackets": true
  },
  "page": 1,
  "itemsPerPage": 25
}
```

| Field | Values |
|---|---|
| `goal` | natural-language graph goal |
| `intent` | `general`, `reachability`, `dependencies`, `symbols` |
| `facets` | `symbols`, `files`, `dependencies`, `relations` |
| `mode` | `plan`, `analyze`, `prove` |
| `maxFiles` | positive integer |
| `subject` | substring matched against graph node id, name, or uri |
| `subjectKind` | `file`, `symbol`, `function`, `class`, `method`, `interface`, `type`, `dependency`, `package`, `entrypoint` |
| `relation` | string or string array, such as `references`, `exports`, `declares`, `imports`, `calls`, `retains` |
| `verdict` | string or string array, such as `candidate-dead`, `transitive-dead`, `reachable`, `candidate-unused-file`, `candidate-unused-dependency` |
| `direction` | `incoming`, `outgoing`, `both` |
| `proof` | `none` or `lsp`; `lsp` runs LSP reference proof for symbol packets on the current page |
| `proofLimit` | max current-page symbols to prove with LSP, capped at 25; default up to 5 |
| `includePackets` | include paged packets with `next.*`; default `true` |
| `includeFacts` | include `why` facts; default `true` |
| `includeEdges` | include relationship edges; default `true` |
| top-level `page` / `itemsPerPage` | page through the filtered packet domain |

The graph row can include `nodes`, `edges`, `facts`, `missingProof`, `packets`,
`nativeGraphSummary`, `graphCapabilities`, `summary`, `graphSummary`, and
`packetPage`.

Graph proof is page-bounded. `params.proof:"lsp"` and `params.mode:"prove"` can
attach `packets[].proof.lsp`, remove `lsp-unavailable` from proved packets, and
change packet `proofStatus`. Unproved pages and paginated LSP results remain
missing proof. Use graph rows to decide exactly what to inspect next, then follow
packet `next.semantic`, `next.fetch`, or `next.search` continuations before
saying code is safe to delete.

### `materialize`

`materialize` takes no `where` and no special `params`.

```json
{
  "schema": "oql",
  "target": "materialize",
  "from": { "kind": "github", "repo": "owner/name", "ref": "main" },
  "scope": { "path": "packages/foo" }
}
```

It returns a materialized checkpoint with fields such as `localPath`, `repoRoot`,
`ref`, `cache`, and `complete`, plus continuations for local structure/files. For
CLI alternatives use `npx octocode clone owner/repo[/path]` or
`npx octocode cache fetch owner/repo [path] --depth file|tree|clone`.

## Materialization And GitHub Index Misses

GitHub code search is provider-index dependent. A zero-result `providerUnindexed`
response from `ghSearchCode` or `target:"code"` over `from.kind:"github"` is NOT
proof that the symbol or file is absent.

Use this recovery order:

```bash
# 1. Verify the remote path exists.
npx octocode search facebook/react/packages/react/src --tree --depth 2

# 2. One-step bounded local proof through search/OQL.
npx octocode search useState packages/react/src --repo facebook/react --materialize required --lang js

# 3. Explicit disk materialization when the next work is multi-file.
npx octocode clone facebook/react/packages/react/src
npx octocode cache fetch facebook/react packages/react/src --depth tree
```

For file-level checks use `npx octocode cache fetch owner/repo path/to/file --depth file`.
For deliberate whole-repo work use `npx octocode clone owner/repo` or
`npx octocode cache fetch owner/repo --depth clone`, but prefer a subtree in large monorepos.
When OQL emits `next.materialize`, follow it directly; it preserves the bounded
`scope.path` from the failed query.

Materialization policy:

```ts
interface MaterializePolicy {
  mode: "never" | "auto" | "required";
  strategy?: "file" | "tree" | "subtree" | "repo";
  allowFullRepo?: boolean;
  forceRefresh?: boolean;
}
```

| Mode | Meaning |
|---|---|
| `never` | Do not clone/cache remote code for local proof. |
| `auto` | Materialize only when required to answer exactly. |
| `required` | Fail if the bounded corpus cannot be materialized. |

The default mode for GitHub sources is `never` (see the Defaults table). Use
materialization when a GitHub provider cannot prove what the user asked:
structural AST search over remote code; PCRE2/local-only regex behavior; file-set
negation; LSP semantics over remote code; or complete local proof for a bounded
subtree. Always bound it with `scope.path`, `scope.include`, or a similarly small
corpus; avoid full-repo materialization unless the user asked and the repo size
is acceptable.

## Views, Select, Controls, And Defaults

Views:

| View | Meaning |
|---|---|
| `discovery` | Smallest output. Prefer paths, identities, and continuations. |
| `paginated` | Default balanced result rows. |
| `detailed` | More context, snippets, and richer payloads. |

`select` projects fields from result rows and continuations:

```json
{ "select": ["path", "line", "snippet", "next.fetch", "next.semantic"] }
```

Projection is most useful on code, file, tree, content, and many record rows.
Some rich record targets can still return a full backing payload; for those,
tighten `params`, `facets`, `maxFiles`, `limit`, and `view`.

Search controls:

```ts
interface QueryControls {
  search?: {
    countLinesPerFile?: boolean;
    countMatchesPerFile?: boolean;
    onlyMatching?: boolean;
    unique?: boolean;
    countUnique?: boolean;
    contextLines?: number;
    invertMatch?: boolean;
    matchWindow?: number;
    matchContentLength?: number;
    maxMatchesPerFile?: number;
    matchPage?: number;
    sort?: "relevance" | "matchCount" | "path" | "modified" | "accessed" | "created";
    sortReverse?: boolean;
    rankingProfile?: string;
    debugRanking?: boolean;
  };
  budget?: {
    maxFiles?: number;
    maxCandidates?: number;
    maxBytes?: number;
    maxMaterializedBytes?: number;
    maxPlanNodes?: number;
    maxBooleanExpansion?: number;
    timeoutMs?: number;
  };
}
```

Defaults:

| Setting | Default |
|---|---|
| `schema` | `oql` |
| `view` | `paginated` |
| `page` | `1` |
| `itemsPerPage` | `25` |
| GitHub `materialize.mode` | `never` |
| GitHub `semantics` materialization | `required`, `strategy:"file"` |
| text case | `smart` |
| local regex dialect | `rust` |
| regex case | `smart` |
| content view | `compact` |
| content character length | `20000` |
| match content length | `500` |
| max plan nodes | `128` |
| max boolean expansion | `64` |
| normal code context | `2` lines |
| detailed code context | `3` lines |
| local search sort | `relevance` |
| local ranking profile | `auto` |

## Batches

Batch up to five independent queries:

```json
{
  "schema": "oql",
  "queries": [
    {
      "target": "semantics",
      "from": { "kind": "local", "path": "./src/index.ts" },
      "params": {
        "type": "references",
        "symbolName": "runCLI",
        "lineHint": 42,
        "includeDeclaration": false
      }
    },
    {
      "target": "semantics",
      "from": { "kind": "local", "path": "./src/index.ts" },
      "params": {
        "type": "references",
        "symbolName": "main",
        "lineHint": 80,
        "includeDeclaration": false
      }
    }
  ],
  "combine": "independent"
}
```

```ts
interface OqlBatch {
  schema?: "oql";
  id?: string;
  queries: OqlQuery[]; // 1-5
  combine?: "independent" | "merge";
}
```

| Field | Values |
|---|---|
| `queries` | 1-5 `OqlQuery` objects |
| `combine` | `independent` or `merge` |

## Normalization And Explain

Raw input can use a small amount of sugar, but canonical OQL is strict. The
normalizer inserts `schema:"oql"`; infers `target` from safe sugar only when
deterministic; rewrites accepted sugar into canonical fields; rejects unknown
fields with `unknownField`; rejects ambiguous sugar with `ambiguousSugar`;
rejects reserved targets with `unsupportedTarget`; validates common target
`params`; and returns the strict normalized query in `--explain`.

`target` inference (deterministic only — otherwise `target` is required):

| If the query has… | inferred `target` |
|---|---|
| `where`, or any predicate sugar (`text`, `regex`, `pattern`, `rule`, `and`/`or`/`xor`/`noneOf`/`oneOf`) | `code` |
| `fetch.content` (and no predicate) | `content` |
| `fetch.tree` (and no predicate) | `structure` |
| `filesWithoutMatch:true` | `files` |
| none of the above | no inference — supply `target` explicitly |

A bare path is NOT enough for the JSON layer to infer a target. (The CLI's
positional shorthand is what turns `npx octocode search ./dir` into a `structure`
read and `npx octocode search ./file.ts` into a `content` read; that lowering happens
in the CLI before OQL, not in `inferTarget`.)

Use `--explain` when a query mixes boolean predicates; a GitHub query may require
materialization; a structural query may not be exact; an answer depends on
negation; or an agent is about to claim absence or safe deletion.

Explain includes `input`, `normalized`, `defaults`, `nodes`, `backendCalls`,
`materialization`, `budgets`, `diagnostics`, and `next`.

Plan routes:

| Route | Meaning |
|---|---|
| `PUSHDOWN` | Backend can evaluate the predicate exactly. No residual work. |
| `RESIDUAL` | Backend narrows candidates, but OQL must finish locally. |
| `ROUTE` | OQL must use another lane, often materialization. |
| `UNSUPPORTED` | OQL cannot execute the requested semantics safely on the chosen source. |

## Result Envelope, Evidence, And Diagnostics

Single-query result:

```ts
interface OqlResultEnvelope {
  queryId?: string;
  queryIndex?: number;
  results: Array<OqlResultRow & { proofGrade: OqlProofGrade }>;
  pagination?: Pagination;
  next?: Record<string, OqlContinuation>;
  diagnostics: OqlDiagnostic[];
  provenance: OqlProvenance[];
  evidence: {
    answerReady: boolean;
    complete: boolean;
    kind: "proof" | "partial" | "candidate" | "unsupported";
  };
  plan?: OqlExplainPlan;
}
```

Result row kinds:

| Row kind | Fields |
|---|---|
| `code` | `proofGrade`, `source`, `path`, `line`, `endLine`, `column`, `snippet`, `metavars`, `metavarRanges`, `next` |
| `file` | `proofGrade`, `source`, `path`, `entryType`, `size`, `modified`, `next` |
| `tree` | `proofGrade`, `source`, `path`, `entryType`, `depth`, `size`, `children`, `next` |
| `content` | `proofGrade`, `source`, `path`, `content`, `range`, `contentView`, `next` |
| `record` | `proofGrade`, `recordType`, `id`, `source`, `data`, `next` |

`proofGrade` is mandatory per row: `candidate`, `text`, `structural`, `semantic`,
`graph`, or `missing`. Projection never removes it.

Record types:

| `recordType` | Payload family |
|---|---|
| `semantics` | LSP operation result |
| `repository` | repository search result |
| `package` | package search result |
| `pullRequest` | PR result |
| `commit` | commit result |
| `artifact` | artifact inspection result |
| `diff` | diff result |
| `research` | smart research flow with summary and paged packets |
| `graph` | relationship graph with nodes, edges, facts, packets, and missing proof |
| `materialized` | materialized checkpoint |

Evidence is part of the answer. `answerReady:true` means the envelope can answer
the query as asked; `answerReady:false` is normal (not a failure) — the returned
rows are valid, but follow `next.*` for more pages, proof, or content.
`complete:true` means the envelope is not missing required pages or proof work.
This single table is the source of truth for evidence semantics:

| Signal | Agent conclusion |
|---|---|
| `answerReady:true` and `complete:true` | You may answer the query as asked. |
| `answerReady:false` | Normal. Results above are valid; follow `next.*` for more pages, LSP proof, or content. |
| `complete:false` | Pages/proof/slices may remain. Follow `next.page`, `next.fetch`, `next.semantic`, `next.search`, `next.graph`, or another returned continuation. |
| `evidence.kind:"proof"` | Backend and OQL routing evaluated the requested semantics exactly. |
| `evidence.kind:"partial"` | Truncation, pagination, or residual checks remain. Report the gap or continue. |
| `evidence.kind:"candidate"` | Report candidates only; do not claim absence or safe deletion. `research`/`graph` are always candidate — upgrade via `next.semantic`/`next.search`/`next.fetch`. |
| `evidence.kind:"unsupported"` | Do not answer as if the query ran. Read diagnostics and repair. |
| `zeroMatches`/`providerUnindexed` on provider search | NOT absence unless `--explain` proves exact bounded evaluation. Verify path with structure, then materialize/clone/cache. |
| `providerSemanticsApproximate` | Useful discovery result, not proof. Materialize or use local/LSP proof for final claims. |
| `proofStatus:"candidate"` | Pre-proof state (no LSP run yet). Run the row's `next.graph` (proof:"lsp") to resolve it. |
| `proofStatus:"conflicting-evidence"` | LSP refs > 0 — the symbol IS retained; inspect `retainedBy`. |
| `proofStatus:"confirmed-by-lsp"` | LSP refs = 0 in the bounded workspace; still check entrypoints, framework conventions, dynamic imports, and package/script exposure before deletion. |
| `proofStatus:"needs-framework-graph"` | LSP alone cannot prove reachability; inspect framework and entrypoint evidence. |

Common diagnostics. Agents must read them; if a diagnostic has
`repair.suggestedQuery`, prefer that over inventing a new shape.

| Code | Meaning |
|---|---|
| `invalidQuery` | Query shape or params are invalid. |
| `unknownField` | Field is not part of OQL. |
| `unsupportedTarget` | Target is reserved or inactive. |
| `unsupportedPredicate` | Predicate cannot run on that target/source. |
| `negativeUniverseRequired` | Negation needs a complete local/materialized universe. |
| `requiresMaterialization` | Exact proof needs materialization. |
| `materializationNotAllowed` | Query needs materialization but mode forbids it. |
| `providerUnindexed` | GitHub provider returned zero rows. NOT absence — verify path, then materialize. |
| `providerSemanticsApproximate` | Provider filter is useful but not exact proof. |
| `vendorNoEquivalent` | A selected backend has no equivalent for the requested OQL intent. |
| `lossyTransform` | A backend mapping would drop or narrow part of the OQL intent. |
| `unsupportedVendorPredicate` | The backend cannot evaluate this predicate; use another target/source or materialize. |
| `responseShapeMismatch` | The backing tool returned a shape the OQL mapper did not understand. |
| `partialResult` | Result is incomplete or candidate-grade. |
| `contentTruncated` | Content was sliced. |
| `matchTruncated` | Match snippet was sliced. |
| `planTruncated` | Explain plan was capped. |
| `budgetExhausted` | Budget stopped the query. |
| `parserFailed` | Parser failed. |
| `partialParse` | Parser skipped or partially parsed inputs (often a missing literal anchor). Non-fatal. |
| `lspUnavailable` | LSP server or capability unavailable. |
| `sanitized` | Secrets or sensitive content were redacted. |
| `rateLimited` | Provider rate limit. |
| `zeroMatches` | No matches. Verify scope before treating as absence. |

## Continuations

Every `next.*` value is an executable OQL query:

```ts
interface OqlContinuation {
  query: OqlCanonicalInput;
  baseQueryId?: string;
  queryIndex?: number;
  why: string;
  confidence: "exact" | "heuristic";
}
```

| Continuation | Meaning |
|---|---|
| `next.fetch` | Read exact content or an outline. |
| `next.semantic` | Ask LSP about the symbol/file. |
| `next.search` | Run a text/regex follow-up. |
| `next.page` | Continue the primary result domain, including research packet pages. |
| `next.matchPage` | Continue per-file match pages when a file has more matches than returned. |
| `next.charRange` | Continue a content range. |
| `next.graph` | Upgrade a research or graph candidate page with bounded graph/LSP proof. |
| `next.structure` | Inspect the tree of a materialized or extracted local path. |
| `next.files` | Enumerate files in a materialized or extracted local path. |
| `next.artifactStrings` | Continue a binary/artifact strings scan. |

Follow continuations because they carry path, range, source, and reasoning
already validated by OQL.

## Research, Graph, And Safe Deletion

`target:"research"` and `target:"graph"` answer reachability and dead-code
questions: "what looks dead, why, what keeps it alive, is that keeper itself
dead, what proof is missing, what to inspect next". Both are candidate-first:
promote candidates with `next.semantic` / `next.search` / `next.fetch` before any
deletion claim. Text and ripgrep are discovery only, never deletion-grade proof.

The full algorithm (structure -> discovery -> AST -> LSP -> graph -> packet),
evidence tiers, verdicts, graph-capability fields, language coverage, and the
question-to-field map are the canonical research contract:
https://github.com/bgauryy/octocode/blob/main/docs/context/OQL_RESEARCH_GRAPH_FLOW.md

OQL beats a single knip-style command when the agent must ask "why?" and continue
into exact proof; a dedicated tool is better for a one-shot, framework-aware
entrypoint/dependency audit.

Recommended two-phase reachability flow:

1. Start with a summary-only research page:

```json
{
  "schema": "oql",
  "target": "research",
  "from": { "kind": "local", "path": "." },
  "params": {
    "intent": "reachability",
    "facets": ["symbols", "files", "relations"],
    "mode": "analyze"
  },
  "page": 1,
  "itemsPerPage": 1
}
```

2. Page candidate packets with the returned `next.page`.
3. Run the row-level `next.graph` exactly as returned; it is page-aligned.
4. Use `params.proof:"lsp"` or the returned graph continuation to attach bounded
   LSP proof to the current packet page.
5. Follow packet-level `next.fetch`, `next.semantic`, and `next.search` for exact
   evidence.
6. Treat `answerReady:false` as normal for candidate research.
7. Only make deletion-grade claims after `proofStatus`, diagnostics, missing
   proof, entrypoints, framework conventions, dynamic imports, and package/script
   exposure all support the conclusion.

### Safe Deletion Rules

For "is this safe to delete?", require:

1. A bounded local or materialized corpus.
2. Export/declaration inventory from regex, AST, or LSP document symbols.
3. LSP references with `includeDeclaration:false`.
4. External-reference classification.
5. Transitive keeper checks for references that point only to other candidate
   dead symbols.
6. File-level import/entrypoint checks.
7. Dependency and script checks for package changes.
8. Review of diagnostics and `missingProof`.

A good OQL answer can say: "candidate dead"; "proof missing: LSP references";
"retained by file X line Y"; "keeper is also unreferenced"; "safe to inspect
next"; "not safe to delete yet". It should NOT say "delete this" when
`evidence.kind:"candidate"` or `answerReady:false`.

### Current Limits To Tell Agents

- `target:"research"` is the right entrypoint for knip-like questions, but the
  current packet is candidate-first. Use `next.semantic`/`next.search`/`next.fetch`
  to upgrade evidence.
- `target:"graph"` is the right entrypoint for retained-by chains. Use
  `params.proof:"lsp"` or `mode:"prove"` for bounded LSP proof on the current
  page, then keep paging/following continuations until missing proof is closed.
- Native graph inventory is capability-driven across supported OXC/tree-sitter
  languages. Read `graphCapabilities`/`nativeGraphSummary` before claiming
  absence; missing capability is not proof of absence.
- `mode:"prove"` on `target:"graph"` is page-bounded. Unproved pages, unavailable
  LSP servers, paginated LSP results, dynamic imports, and framework entrypoints
  remain `missingProof`.
- Tree-sitter graph facts are syntax inventory; public/export hints and call
  edges are language-aware candidates, not semantic proof. LSP references,
  definitions, and call hierarchy are the proof layer.
- Rich `research`/`graph` rows can still return packet-shaped data. Keep `facets`,
  `maxFiles`, `page`, `itemsPerPage`, and `view` tight.
- Structural AST search is exact only when the pattern/rule is accepted by the
  parser and diagnostics are clean.
- File and dependency deletion still need project-specific entrypoint, framework,
  script, dynamic import, and generated-file awareness.
- LSP proof is only as complete as the workspace, language server, and symbol
  anchor provided.

## Agent Rules And Checklist

Agents should follow these rules:

- Choose one target first.
- Never use `where` with `content` or `structure`.
- Never use `params` for ordinary code/file matching.
- Prefer `view:"discovery"` for orientation.
- Use `select` aggressively, then tighten target params if a rich record target
  still returns more than needed.
- Use `--explain` before claiming absence, dead code, or safe deletion.
- Treat provider zero results as absence only when the plan proves the provider
  evaluated the exact predicate over the needed universe.
- Treat `research` output as candidate evidence until proof continuations are
  followed.
- Follow `next.*` continuations instead of inventing paths, line ranges, pages,
  offsets, or symbol anchors.
- Read diagnostics before answering.
- Cite file paths and lines from proof-grade sources whenever possible.

One-screen checklist before answering:

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

## Appendix: Transformer Architecture (contributor-only / internal)

This appendix is internal/contributor documentation. You do NOT need any of it to
use OQL — it describes how OQL maps canonical intent onto provider/tool backends.
Skip it unless you are implementing or debugging a transformer. Parts of it
describe a target architecture: a desired end state where every adapter mapping
is first-class and explainable, even where today's code still performs some
mapping inside an adapter module.

OQL is one canonical language over many provider APIs. A transformer is the
boundary that translates between those two worlds:

```text
OQL query
  -> transformer for target + source
  -> provider/tool-specific query
  -> provider/tool-specific response
  -> transformer back to OQL rows, diagnostics, pagination, and evidence
```

Transformers keep the public OQL shape stable while GitHub, npm, local search,
LSP, binary inspection, and future providers keep their own vocabulary. Agents
write OQL intent; transformers decide how that intent maps to the vendor or local
primitive.

Current implementation note: OQL has a transformer registry for active
source/target lanes. Each entry declares the adapter/backend contract; some
field-level lowering still lives inside the adapter module named by the registry.
The remaining work is to move all field-by-field lowering, loss diagnostics,
pagination mapping, and result-shape checks behind those registered transformer
contracts.

Examples:

| OQL intent | GitHub code search | GitHub repo/PR search | Local search | npm |
|---|---|---|---|---|
| `scope.language:"ts"` | Prefer `extension:"ts"` for file-level code search. | Use `language:"TypeScript"` only when filtering repository language is intended. | Use TypeScript file/type filters. | No direct field; emit a diagnostic or defer to source-repo follow-up. |
| `scope.language:"typescript"` | Use `language:"TypeScript"` or expand to TypeScript extensions when exact file types are needed. | Use `language:"TypeScript"`. | Expand to the TypeScript extension family. | No direct field. |
| `where.kind:"text"` | `keywords` / provider text query. | PR/repo keyword query when the target supports it. | Ripgrep text search. | Package-name or keyword search when target is `packages`. |
| `where.kind:"structural"` | Not native; materialize first for local AST proof. | Not native. | Structural engine query. | Not native. |
| `fetch.content.contentView:"symbols"` | `ghGetFileContent` with symbol minification. | Not a repo/PR-list field. | `localGetFileContent` symbol view. | Not native. |

Transformers have two separate jobs: query transformation (convert canonical OQL
fields into the best provider query without dropping meaning silently) and result
transformation (convert provider output back into OQL rows, with stable `kind`,
`recordType`, `path`, pagination, `next`, diagnostics, and evidence). Keep them
separate: a query can be transformed correctly while the result shape changes
underneath it; that must become a `responseShapeMismatch` bug, not an empty
research answer.

Transformer diagnostics must make lossy mappings visible:

| Diagnostic | Meaning |
|---|---|
| `vendorNoEquivalent` | The OQL selector has no direct backend field. |
| `lossyTransform` | The backend query is valid but weaker than the OQL intent. |
| `unsupportedVendorPredicate` | The selected backend cannot evaluate the predicate; materialization or a different target is required. |
| `responseShapeMismatch` | The backing tool returned a shape the transformer did not understand. |

Never confuse provider dialect with OQL meaning. `scope.language:"ts"` expresses
language/extension intent; one backend may map it to an exact `.ts` extension,
another to a broader TypeScript language family, another may have no direct
constraint. The transformer owns that decision and must expose it in `--explain`,
diagnostics, or provenance when the mapping is approximate.

### Transformer Contract

Each transformer must be a first-class component with the same contract. Do not
hide provider-specific behavior inside the CLI parser, renderer, or ad hoc adapter
branches.

| Contract part | Required behavior |
|---|---|
| Capability declaration | State which OQL targets, sources, predicates, scope fields, fetch modes, params, and controls the backend can evaluate. |
| Input lowering | Map canonical OQL fields to backend fields, including provider naming differences such as `scope.language` -> extension filters, provider language filters, backend language-family parameters, or include globs. |
| Exactness model | Mark each lowered field as exact, approximate, residual, routed, or unsupported. |
| Loss diagnostics | Emit `lossyTransform`, `vendorNoEquivalent`, `unsupportedVendorPredicate`, `requiresMaterialization`, or a more specific diagnostic before any meaning is dropped. |
| Pagination mapping | Normalize backend pagination into OQL `pagination` and `next.page`; preserve secondary domains such as per-file match pages, char windows, PR file pages, artifact scan offsets, and research packet pages. |
| Minification/content mapping | Map OQL `contentView` (`exact`, `compact`, `symbols`) to backend minify modes and report truncation/sanitization. |
| Output projection | Map backend data into stable OQL rows: `code`, `file`, `tree`, `content`, or `record` with the right `recordType`. |
| Continuation mapping | Attach executable `next.*` queries for exact reads, semantic proof, char ranges, match pages, materialization, graph proof, artifact scans, and structure/files follow-ups. |
| Error mapping | Convert backend errors and empty/provider-index ambiguity into typed OQL diagnostics with repair hints when possible. |
| Explain trace | Show `oql.path -> backend.path`, exactness, dropped fields, fallback routes, materialization, and result-shape expectations in `--explain`. |

### Language Selector Logic

`scope.language` is canonical OQL intent, not a backend field. It may describe a
language family (`typescript`) or an exact extension (`ts`). Transformers decide
how to project it:

| Selector | GitHub code search | GitHub repository search | Local ripgrep/code search | Local file discovery | Structural AST | LSP |
|---|---|---|---|---|---|---|
| `ts` | `extension:"ts"` | repository `language:"TypeScript"` only when target is `repositories` | include `**/*.ts` or exact file-type filter | basename/include glob `*.ts` | include glob `*.ts` plus structural `lang` when supplied | file extension helps choose the TS server; semantic op still needs a file/uri |
| `tsx` | `extension:"tsx"` | repository `language:"TypeScript"` only for repo discovery | include `**/*.tsx` | basename/include glob `*.tsx` | include glob `*.tsx` plus structural `lang` | TS/TSX language server |
| `typescript` | `language:"TypeScript"` can be lossy for file-complete proof because provider language coverage may not equal all known TS extensions | `language:"TypeScript"` | TypeScript extension family | `*.ts`, `*.tsx`, `*.mts`, `*.cts` | structural include globs for known TS extensions | TS server; still needs symbol/file anchors |
| unknown selector | pass only if backend accepts it exactly, otherwise diagnose | pass only if provider accepts it | prefer explicit include globs or diagnose | prefer explicit globs or diagnose | diagnose unless parser language is known | diagnose or route to content/search first |

Rules:

- Exact extension selectors (`ts`, `.tsx`, `py`) stay exact when a backend
  supports extension filtering.
- Language-family selectors (`typescript`, `javascript`, `python`) can expand to
  known extensions locally.
- If a provider language filter cannot cover every known extension, emit
  `lossyTransform` or require materialization for proof.
- Multiple `scope.language` values must not be silently dropped. Either lower all
  values exactly, run multiple backend calls, or emit a blocking diagnostic.
- Unknown language selectors are never proof of absence.

### Supporting First-Class Components

The transformer architecture is more than one backend adapter per API. These
compiler pieces are also first-class transformer infrastructure:

| Component | Owns | Why agents need it |
|---|---|---|
| Canonicalizer + target-param validator | Sugar lowering, strict canonical OQL shape, ambiguity errors, common `params` validation | `--explain` must show the exact canonical query that ran. |
| Language selector transformer | `scope.language` projection into extension filters, provider language filters, local language-family parameters, include globs, structural globs, LSP file/server hints | One language intent, precise per-backend naming + loss diagnostics. |
| Capability planner + lossiness router | `PUSHDOWN`, `RESIDUAL`, `ROUTE`, `UNSUPPORTED`, backend choice, exactness, materialization requirements, provider approximation | Tell whether a result is proof, candidate, routed, or impossible. |
| Predicate compiler | `where` text/regex/structural/field/boolean into backend query knobs or local set algebra | Express conditions once without guessing ripgrep/AST/GitHub/file-search flags. |
| Result row mapper | Backend payloads into stable `code`, `file`, `tree`, `content`, `record` rows | One result shape even when providers return incompatible payloads. |
| Pagination mapper | Backend pages, char windows, per-file match pages, PR/file/comment pages, artifact scan offsets, research packet pages into OQL pagination/continuations | Know which result domain still has data and which continuation to run. |
| Evidence/envelope builder | `evidence.kind`, `answerReady`, `complete`, row `proofGrade`, diagnostics, provenance | Decide whether to answer, continue, or report candidate-only findings. |
| Continuation builder | `next.fetch`, `next.semantic`, `next.graph`, `next.page`, `next.matchPage`, `next.charRange`, `next.artifactStrings`, `next.structure`, `next.files` | Follow validated next steps instead of inventing paths, anchors, pages, or proof queries. |

### Required Transformer Inventory

The current code has active registry entries for the source/target pairs below.
Each entry owns the explain/provenance contract and points at the backend plus
adapter functions that perform the detailed input/output mapping.

| Transformer | State | Must map |
|---|---|---|
| `github.code -> ghSearchCode` | Active | `from.repo/owner/ref`, `scope.path`, `scope.language`, text/regex provider predicates, `params.extension`, `params.filename`, `params.match`, `limit/page`, provider-index diagnostics, path-level code rows, `next.fetch`, lossy language/path diagnostics. |
| `github.files -> ghSearchCode` | Active | File-containing-term queries, `match:"file"`, deduped file rows, approximate provider semantics, materialization repair for exact file sets, pagination. |
| `github.content -> ghGetFileContent` | Active | `fetch.content.contentView` -> `minify`, range/context, match string/regex/case, char windows, branch/ref, content rows, `next.charRange`, truncation/sanitization diagnostics. |
| `github.structure -> ghViewRepoStructure` | Active | `scope.path`, `fetch.tree.maxDepth`, sizes, repo/ref, tree rows, pagination, provider-empty diagnostics. |
| `github.semantics -> ghCloneRepo + lspGetSemantics` | Active | Remote semantic operations via sparse materialization followed by local LSP; `params.uri`, operation `type`, symbol anchors, materialization provenance, semantic record rows. |
| `github.repositories -> ghSearchRepos` | Active | `params.keywords`, `topicsToSearch`, `language`, owner, stars/size/updated/license/visibility/archived/sort/page, repository record rows, provider pagination, language selector mapping. |
| `github.pullRequests -> ghHistoryResearch` | Active | PR list/detail, `prNumber`, state, author, labels, branch filters, keyword search, file/comment/commit pages, patch char windows, match scopes, PR record rows, secondary pagination and `next.*`. |
| `github.commits -> ghHistoryResearch` | Active | path, branch/ref, since/until, includeDiff, commit/file pagination, commit rows, diff continuations, not-found/rate-limit diagnostics. |
| `github.diff.prPatch -> ghHistoryResearch` | Active | PR patch lane (`prNumber`, `files`), diff rows, file-page/patch-page continuations, invalid-lane repair. |
| `github.diff.directFile -> ghGetFileContent` | Active | Direct two-ref file lane (`baseRef`, `headRef`, `path`), content diff rows (real line diff), invalid-lane repair. |
| `github.materialize -> ghCloneRepo/cache` | Active | bounded repo/subtree clone, `scope.path/include/exclude`, force refresh, allow-full-repo guard, materialized checkpoint rows, `next.structure` and `next.files`. |
| `local.code.textRegex -> localSearchCode/ripgrep` | Active | text/regex predicates, case/whole-word/multiline/dotall/fixed/PCRE2 flags, include/exclude/hidden/noIgnore, match windows, only-matching, counts, ranking/sort, per-file match paging, `matchTruncated`, code rows. |
| `local.code.structural -> localSearchCode structural` | Active | structural `pattern` or YAML `rule`, parser language, include globs, metavars/ranges, structural proof grade, parser/partial diagnostics, materialized/local-only capability. |
| `local.files -> localFindFiles` | Active | field predicates over path/basename/extension/size/modified/entryType, content-backed files routed through local search when needed, negative file sets, include/exclude/depth/hidden/noIgnore, local universe proof, file rows, pagination. |
| `local.content -> localGetFileContent` | Active | exact/compact/symbol content views, ranges, context, match anchors, char windows, full content, content truncation, sanitized output, `next.charRange`. |
| `local.structure -> localViewStructure` | Active | tree depth, sizes, hidden, path scope, tree rows, pagination, structure continuations. |
| `local.semantics -> lspGetSemantics` | Active | operation `type`, uri/path, symbolName, lineHint, orderHint, workspaceRoot, depth, includeDeclaration, groupByFile, workspaceSymbol, supertypes/subtypes, diagnostics, LSP unavailable/capability diagnostics, semantic record rows, `next.fetch`. |
| `local.research -> OQL research analyzer` | Active | goal/intent/facets/mode/maxFiles, full-scope summary, packet-domain pagination, graph capabilities, native graph facts, packet continuations, candidate evidence. |
| `local.graph -> OQL graph analyzer + LSP proof` | Active | subject/relation/verdict/direction/proof/proofLimit/include flags, nodes/edges/facts/packets, missingProof, page-bounded LSP proof, proofStatus, `next.graph`, `next.fetch`, `next.semantic`. |
| `npm.packages -> npmSearch` | Active | packageName vs keywords, mode lean/full, page, package rows, source repository hints, npm pagination/errors, follow-up repository/materialize continuations. |
| `local.artifacts -> localBinaryInspect` | Active | inspect/list/extract/decompress/strings/unpack, archive entry pages, string scan offsets, char windows, minLength/matchString/verbose, artifact rows, `next.artifactStrings`, extraction/materialized local continuations. |

Registry rules:

- Every active target/source pair either has a transformer or a deliberate
  `unsupportedTarget` / `unsupportedPredicate` diagnostic with a repair.
- `search --scheme` and docs should be generated from transformer metadata where
  possible.
- New provider APIs are added by implementing a transformer, not by changing the
  public OQL language.
- Adapters may call backing tools, but they should not invent new OQL meaning that
  the transformer registry cannot explain.
