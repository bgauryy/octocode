# OQL Language Reference

Full language specification for the Octocode Query Language. The executable
contract always wins: `npx octocode search --scheme`. If this doc and `--scheme`
ever disagree, `--scheme` is correct — open an issue.

**Quick reference:** [OCTOCODE_QUERY_LANGUAGE.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OCTOCODE_QUERY_LANGUAGE.md) — cheatsheet, decision tree, common recipes, agent rules.
**Results and evidence:** [OQL_RESULTS_AND_EVIDENCE.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode/docs/OQL_RESULTS_AND_EVIDENCE.md) — envelopes, diagnostics, continuations, safe deletion.

---

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

| Field | Role | Notes |
|---|---|---|
| `target` | What kind of answer to return | Choose this first. |
| `from` | Which corpus or provider to use | Local path, GitHub scope, materialized path, or npm. |
| `scope` | Bounds inside the corpus | Path, language, include/exclude globs, depth. Bound every expensive or absence-sensitive query. |
| `where` | Code/file predicate for matching | Text, regex, structural AST, file field, boolean combinations. Used by `code` and `files` only. |
| `params` | Target-specific options | LSP type, repo filters, package query, PR number, artifact mode, research goal, etc. |
| `fetch` | Read instructions | Exact content, compact content, symbol outline, tree settings. Used by `content` and `structure`. |
| `materialize` | Remote-to-local policy | Allow or require bounded GitHub materialization for local proof. |
| `select` | Field projection | Return only the fields an agent needs. |
| `view` | Result density | `discovery` (paths only), `paginated` (default), `detailed` (rich). |
| `controls` | Output/cost controls | Match windows, max matches, budgets, sort, ranking. |
| `limit` | Total cap | Cap total returned results where the target supports it. |
| `page` | Result page | Top-level page number. Follow `next.page`. |
| `itemsPerPage` | Page size | Page the target's primary result domain. |
| `explain` | Routing visibility | Include normalized query, plan, backend calls, diagnostics. |

Plain-language mapping:

| General idea | OQL field |
|---|---|
| Where to look | `from` |
| What kind of answer to return | `target` |
| Bounds inside that source | `scope` |
| Match/filter conditions | `where` |
| What to read once a file/tree is known | `fetch` |
| Options specific to one answer type | `params` |
| Response shape, projected fields, cost limits | `view`, `select`, `controls` |
| Paging the target's primary result domain | `page`, `itemsPerPage` |

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

---

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
| `diff` | `recordType:"diff"` | PR patch diff or two-ref file diff | `ghHistoryResearch` or content diff |
| `research` | `recordType:"research"` | Smart local research packets | OQL research analyzer |
| `graph` | `recordType:"graph"` | Relationship nodes, edges, facts, and missing proof | OQL graph analyzer |
| `materialize` | `recordType:"materialized"` | Clone/cache a bounded GitHub corpus | `ghCloneRepo` and cache lanes |

Reserved targets (`fixes`, `dataflow`) return `unsupportedTarget`. Do not use them.

---

## Sources

```ts
type QuerySource =
  | { kind: "local"; path: string }
  | { kind: "github"; repo?: string; owner?: string; ref?: string }
  | { kind: "materialized"; localPath: string; source?: QuerySource }
  | { kind: "npm" };
```

Rules:
- `local.path` is a file or directory on disk. The pre-filled `next.fetch` carries the resolved ABSOLUTE path — follow it rather than re-joining paths.
- A `local`/`materialized` path that does not exist on disk returns a blocking `invalidQuery` diagnostic (`answerReady:false`), NOT an empty proof. A typo'd path cannot be mistaken for "confirmed absent."
- `github.repo` is `"owner/name"`; `ref` is optional.
- `github.owner` for provider discovery targets.
- `materialized.localPath` is a local checkout returned by `target:"materialize"` or clone/cache flows.
- `npm` is for package registry discovery.
- `packages` and `repositories` can run without a local code corpus.
- `content` and `structure` over GitHub need a concrete repository.

---

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
differently — extension filters, provider language filters, include globs, or LSP
hints. Unknown language selectors are never proof of absence.

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

---

## Predicates

`where` is a discriminated predicate tree, used only by `code` and `files`.
- `target:"code"` requires a `where` predicate.
- `target:"content"` and `target:"structure"` reject `where` — use `fetch` instead.

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

Default: local regex uses the Rust dialect.

### Structural AST

```json
{
  "kind": "structural",
  "lang": "typescript",
  "pattern": "function $NAME($$$ARGS): $RET { $$$BODY }"
}
```

Or with a JSON rule object:

```json
{
  "kind": "structural",
  "lang": "typescript",
  "rule": { "pattern": "eval($X)" }
}
```

Or with a YAML rule string:

```json
{
  "kind": "structural",
  "lang": "typescript",
  "rule": "rule:\n  pattern: \"eval($X)\""
}
```

| Field | Values |
|---|---|
| `lang` | Required language id: `typescript`, `javascript`, `python`, `rust`, etc. |
| `pattern` | Code-shaped AST pattern |
| `rule` | Relational AST rule — JSON object or grep-compatible YAML rule string |

Use exactly one of `pattern` or `rule`.

**Structural patterns must match the COMPLETE AST node.** A function WITH a
return type only matches if the pattern has one too (`function $N($$$A): $R { $$$B }`).
Useful shapes:
- `function $N($$$A) { $$$B }` — no-return-type function
- `($$$A) => $$$B` — arrow function
- `$F($$$A)` — call expression
- `$O.$M($$$A)` — method call

For "find symbol X" use a rule, not a bare pattern:
`{ "kind": "function_declaration", "has": { "pattern": "X" } }`

0 matches with a `partialParse`/`partialResult` diagnostic means the pattern
shape doesn't match the real node. Fall back to: a narrower rule;
`target:"semantics"` with `documentSymbols`; a regex export inventory.

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

Field predicates run on `target:"files"`. A bare field predicate on
`target:"code"` is rejected with `unsupportedPredicate`. Over a GitHub source,
only path-like equality (`path`/`basename`/`extension` with `op:"="`) pushes
down to the provider.

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
negative predicates. Provider search often cannot prove absence without
materialization.

### Boolean Sugar

Top-level sugar keys accepted beside `where`, lowered to canonical booleans at normalize time:

| Sugar | Lowers to |
|---|---|
| `and: [P1, P2, …]` | `{ kind:"all", of:[…] }` |
| `or: [P1, P2, …]` | `{ kind:"any", of:[…] }` |
| `noneOf: [P1, P2, …]` | `{ kind:"not", predicate:{ kind:"any", of:[…] } }` |
| `xor: [P1, P2]` (exactly two) | `any(all(P1, not P2), all(not P1, P2))` |
| `oneOf: [P1, …, Pn]` | exactly-one expansion (bounded by `controls.budget.maxBooleanExpansion`, default 64) |
| `invert: true` | wraps the whole predicate in `not` |

```json
{
  "target": "code",
  "from": { "kind": "local", "path": "./src" },
  "and": [
    { "kind": "text", "value": "useEffect" },
    { "kind": "text", "value": "useState" }
  ]
}
```

Prefer canonical `all`/`any`/`not` in programmatic queries. `oneOf`/`xor`
expansions count against the boolean-expansion budget.

---

## Fetch and Content Views

`fetch` reads content or trees. It does not search. Use only with `content` or
`structure` targets.

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
| `exact` | Exact source text — best for citations and patches. |
| `compact` | Minified/compact content — default, most token-efficient. |
| `symbols` | File outline or symbol skeleton — best for orientation. |

To read the region around a string, use `fetch.content.match` (NOT a top-level
`where`). If a content result reports `sanitized`, prefer `match.text` anchors
over hard-coded line math.

Examples:

```json
{
  "target": "content",
  "from": { "kind": "local", "path": "./src/index.ts" },
  "fetch": { "content": { "range": { "startLine": 1, "endLine": 80 }, "contentView": "exact" } }
}
```

```json
{
  "target": "content",
  "from": { "kind": "local", "path": "./src/index.ts" },
  "fetch": { "content": { "match": { "text": "runCLI" }, "contentView": "exact" } }
}
```

---

## Params by Target

`params` is for target-specific options. OQL validates common fields early; the
backing tool remains the exhaustive validator. Fields accepted only by an internal
pass-through are not part of the agent contract until the scheme lists them.

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
| `uri` | Optional file uri/path override |
| `symbolName` | Symbol to resolve near `lineHint`; fuzzy search query for `workspaceSymbol` |
| `symbolKind` | Filter returned symbol rows |
| `lineHint` | 1-based line anchor (required except for `documentSymbols`, `workspaceSymbol`, `diagnostic`) |
| `orderHint` | Integer disambiguation hint |
| `includeDeclaration` | boolean |
| `depth` | integer 0-20 |
| `contextLines` | call-flow snippet context for `callers`, `callees`, `callHierarchy` |
| `groupByFile` | boolean |
| `workspaceRoot` | Optional workspace root |
| `format` | `structured` or `compact` |

LSP 3.17 additions:

| Type | Anchor | Use when |
|---|---|---|
| `workspaceSymbol` | `symbolName` only (no file or position) | Find all symbols matching a name project-wide |
| `supertypes` | `uri` + `symbolName` + `lineHint` | Walk up an inheritance chain |
| `subtypes` | `uri` + `symbolName` + `lineHint` | Walk down an inheritance chain |
| `diagnostic` | `uri` only | Pull errors/warnings for a file from the language server (LSP pull model) |

`diagnostic` requires a language server supporting the pull-diagnostic protocol
(`textDocument/diagnostic`, LSP 3.17). Others return `unsupportedOperation`.

When `from.kind:"github"`, OQL sparsely materializes the file then runs LSP
locally. Normalization reports `materialize:{mode:"required",strategy:"file"}` in
`--explain` even if the caller omitted materialization.

For deletion/reachability: use `references` with `includeDeclaration:false`.

### `repositories`

Backs onto `ghSearchRepos`.

| Field | Values |
|---|---|
| `keywords` | string array |
| `topicsToSearch` | string array |
| `language` | string |
| `owner` | string |
| `stars` | string or number range |
| `license` | string |
| `archived` | boolean |
| `sort` | `stars`, `forks`, `help-wanted-issues`, `updated`, `best-match` |
| `limit` | positive integer |
| `page` | positive integer |

CLI extra flags: `--forks`, `--created`, `--updated`, `--size`,
`--visibility`, `--good-first-issues`. See `npx octocode search --help`.

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
| `matchString` | content filter over fetched PR title/body/comments/reviews |
| `matchScope` | `body`, `title`, `comments`, `reviews`, `all` |
| `limit`, `page` | pagination |

### `commits`

Backs onto `ghHistoryResearch` with commit mode.

| Field | Values |
|---|---|
| `path` | optional path filter |
| `branch` | optional branch |
| `since`, `until` | date strings |
| `includeDiff` | boolean |
| `matchString` | filters commit messages |
| `filePage`, `itemsPerPage` | changed-file pagination for commit diffs |
| `limit`, `page` | pagination |

### `diff`

PR patch lane:

```json
{
  "target": "diff",
  "from": { "kind": "github", "repo": "owner/name" },
  "params": { "prNumber": 123, "files": ["src/index.ts"] }
}
```

Direct file lane (two-ref, produces a real line diff):

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

### `research`

Returns a smart local research packet. Candidate-first.

```json
{
  "target": "research",
  "from": { "kind": "local", "path": "." },
  "params": {
    "goal": "what looks dead, why, what keeps it alive, and what proof is missing?",
    "intent": "reachability",
    "facets": ["symbols", "files", "dependencies", "relations"],
    "mode": "analyze",
    "maxFiles": 200
  },
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
| `page` / `itemsPerPage` | page through the generated packet list |

Modes:

| Mode | Meaning |
|---|---|
| `plan` | Return the research flow without scanning files. |
| `analyze` | Return candidate summary, graph packets, and continuations. |
| `prove` | Require explicit `params.intent`; return candidate-grade packets with required `next.*` proof steps. |

Research packets use native AST facts (OXC for JS/TS, tree-sitter for others).
`graphCapabilities` tells agents which extensions can emit graph facts and whether
any source files missed graph extraction.

### `graph`

Returns an agent-readable relationship graph enriched by native AST graph facts.

```json
{
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
| `relation` | `references`, `exports`, `declares`, `imports`, `calls`, `retains` (or string array) |
| `verdict` | `candidate-dead`, `transitive-dead`, `reachable`, `candidate-unused-file`, `candidate-unused-dependency` (or string array) |
| `direction` | `incoming`, `outgoing`, `both` |
| `proof` | `none` or `lsp` — `lsp` runs LSP reference proof for symbol packets on the current page |
| `proofLimit` | max current-page symbols to prove with LSP, capped at 25; default 5 |
| `includePackets` | include paged packets with `next.*`; default `true` |
| `includeFacts` | include `why` facts; default `true` |
| `includeEdges` | include relationship edges; default `true` |
| `page` / `itemsPerPage` | page through the filtered packet domain |

Graph proof is page-bounded. `params.proof:"lsp"` can attach `packets[].proof.lsp`,
remove `lsp-unavailable`, and change `proofStatus`. Unproved pages remain missing
proof. Follow packet `next.semantic`, `next.fetch`, or `next.search` before
claiming code is safe to delete.

### `materialize`

Takes no `where` and no special `params`.

```json
{
  "target": "materialize",
  "from": { "kind": "github", "repo": "owner/name", "ref": "main" },
  "scope": { "path": "packages/foo" }
}
```

Returns a materialized checkpoint with `localPath`, `repoRoot`, `ref`, `cache`,
and `complete`, plus continuations for local structure/files.

CLI alternatives:
```bash
npx octocode clone owner/repo[/path]
npx octocode cache fetch owner/repo [path] --depth file|tree|clone
```

---

## Materialization and GitHub Index Misses

A zero-result `providerUnindexed` response from `ghSearchCode` is NOT proof of
absence.

Recovery order:

```bash
# 1. Verify the remote path exists
npx octocode search vercel/next.js/packages/next/src --tree --depth 2

# 2. One-step bounded local proof
npx octocode search useState packages/next/src --repo vercel/next.js --materialize required --lang ts

# 3. Explicit materialization when next work is multi-file
npx octocode clone vercel/next.js/packages/next/src
npx octocode cache fetch vercel/next.js packages/next/src --depth tree
```

When OQL emits `next.materialize`, follow it directly — it preserves the bounded
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
| `never` | Do not clone/cache remote code. |
| `auto` | Materialize only when required to answer exactly. |
| `required` | Fail if the bounded corpus cannot be materialized. |

Default for GitHub sources is `never`. Use materialization for: structural AST
search over remote code; PCRE2/local-only regex; file-set negation; LSP semantics
over remote code; complete local proof for a bounded subtree. Always bound with
`scope.path`, `scope.include`, or a similarly small corpus.

---

## Views, Select, Controls, and Defaults

Views:

| View | Meaning |
|---|---|
| `discovery` | Smallest output — prefer paths, identities, and continuations. |
| `paginated` | Default balanced result rows. |
| `detailed` | More context, snippets, and richer payloads. |

`select` projects fields from result rows and continuations:

```json
{ "select": ["path", "line", "snippet", "next.fetch", "next.semantic"] }
```

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

---

## Batches

Batch up to five independent queries:

```json
{
  "schema": "oql",
  "queries": [
    {
      "target": "semantics",
      "from": { "kind": "local", "path": "./src/index.ts" },
      "params": { "type": "references", "symbolName": "runCLI", "lineHint": 42, "includeDeclaration": false }
    },
    {
      "target": "semantics",
      "from": { "kind": "local", "path": "./src/index.ts" },
      "params": { "type": "references", "symbolName": "main", "lineHint": 80, "includeDeclaration": false }
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

---

## Normalization and Explain

The normalizer: inserts `schema:"oql"`; infers `target` from safe sugar only
when deterministic; rewrites sugar into canonical fields; rejects unknown fields
with `unknownField`; rejects ambiguous sugar with `ambiguousSugar`; rejects
reserved targets with `unsupportedTarget`; validates common target `params`.

`target` inference (deterministic only — otherwise `target` is required):

| If the query has… | Inferred `target` |
|---|---|
| `where`, or any predicate sugar (`text`, `regex`, `pattern`, `rule`, `and`/`or`/`xor`/`noneOf`/`oneOf`) | `code` |
| `fetch.content` (and no predicate) | `content` |
| `fetch.tree` (and no predicate) | `structure` |
| `filesWithoutMatch:true` | `files` |
| none of the above | no inference — supply `target` explicitly |

A bare path does NOT infer a target in the JSON layer. (The CLI shorthand turns
`./dir` into a structure read and `./file.ts` into a content read; that lowering
happens in the CLI before OQL, not in `inferTarget`.)

Use `--explain` when: a query mixes boolean predicates; a GitHub query may
require materialization; a structural query may not be exact; an answer depends on
negation; or an agent is about to claim absence or safe deletion.

`--explain` includes: `input`, `normalized`, `defaults`, `nodes`, `backendCalls`,
`materialization`, `budgets`, `diagnostics`, and `next`.

Plan routes:

| Route | Meaning |
|---|---|
| `PUSHDOWN` | Backend can evaluate the predicate exactly. No residual work. |
| `RESIDUAL` | Backend narrows candidates, but OQL must finish locally. |
| `ROUTE` | OQL must use another lane, often materialization. |
| `UNSUPPORTED` | OQL cannot execute the requested semantics safely on the chosen source. |
