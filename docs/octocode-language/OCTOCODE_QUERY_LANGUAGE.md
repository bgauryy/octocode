# Octocode Query Language (OQL)

OQL is the typed research query object behind `octocode search`. One JSON shape,
one runner, one result envelope — for searching code, reading files, browsing
trees, asking LSP questions, and researching repositories, packages, PRs,
commits, artifacts, diffs, and dead code, over local paths and GitHub.

OQL does **not** replace the underlying tools. It compiles into the existing
Octocode runners and returns typed results, diagnostics, provenance, evidence,
and executable follow-up queries (`next.*`).

This document is the canonical reference for the language. It is written to be
read by both people and agents: start at the top for the mental model, jump to a
section for exact rules and parameters.

- Implementation lives in `packages/octocode-tools-core/src/oql` (`runOqlSearch`).
- Per-target replacement status: see
  [OCTOCODE_SEARCH_PARITY_CHECKLIST.md](https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_SEARCH_PARITY_CHECKLIST.md).
- Sequencing / future work: see
  [OCTOCODE_QUERY_LANGUAGE_PLAN.md](https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_QUERY_LANGUAGE_PLAN.md).

---

## 1. One screen

```jsonc
{
  "schema": "oql",
  "target": "code",
  "from":  { "kind": "local", "path": "./src" },
  "scope": { "language": ["ts"], "excludeDir": ["node_modules", "dist"] },
  "where": { "kind": "text", "value": "runCLI" },
  "select": ["path", "line", "snippet", "next.fetch"],
  "view": "paginated",
  "limit": 25
}
```

Run it:

```bash
octocode search --query '<JSON>' --json
octocode search --explain --dry-run --query '<JSON>'   # plan only, no execution
```

A query has up to twelve fields. You rarely use all of them:

| Field | Answers | Use when |
|---|---|---|
| `target` | What kind of result? | Always — pick one target first. |
| `from` | Where is the corpus? | Local path, GitHub repo, materialized path, npm. |
| `scope` | What subset is allowed? | Paths, languages, globs, depth, hidden/ignore. |
| `where` | What code/file predicate must match? | `code` and `files` only. |
| `params` | What operation should the target run? | LSP, repo, package, PR, commit, artifact, diff, research. |
| `fetch` | What content/tree to read? | `content` and `structure`. |
| `materialize` | May remote code be cloned for local proof? | GitHub AST/PCRE2/LSP/exact/negation. |
| `select` | Which fields to return? | Keep output small. |
| `view` | How dense? | `discovery` / `paginated` (default) / `detailed`. |
| `controls` | Cost/paging/ranking limits. | Match pages, only-matching, budgets. |
| `limit` / `page` / `itemsPerPage` | Result bounds. | Cap and page result rows. |
| `explain` | Show the plan? | When routing or proof strength is unclear. |

**Pick the right lane:**

| Need | Use |
|---|---|
| Find text / regex / AST code matches | `target:"code"` + `where` |
| Find files by name/path/metadata, or files that (don't) contain a term | `target:"files"` + `where` |
| Read a file, range, match window, or symbol skeleton | `target:"content"` + `fetch.content` |
| Browse a directory or repo tree | `target:"structure"` + `fetch.tree` |
| Ask LSP for symbols / refs / defs / calls / hover | `target:"semantics"` + `params` |
| Search repos / packages / PRs / commits / artifacts / diffs | matching `target` + `params` |
| Smart repo research (dead code, dependency drift) | `target:"research"` + `params` |
| Clone/cache a bounded GitHub area for local proof | `target:"materialize"` |

---

## 2. Runner protocol

`octocode search` is a thin CLI wrapper over `runOqlSearch`. The MCP OQL tool
serves the same schema and execution path.

| Command | Meaning |
|---|---|
| `octocode search --scheme` | Print the OQL input and result schema. |
| `octocode search --query JSON` | Run one OQL query object. |
| `octocode search --file ./q.json` | Run an OQL query from a file. |
| `… | octocode search --stdin` | Run an OQL query from stdin. |
| `octocode search --explain --query JSON` | Include the normalized query + plan in the result. |
| `octocode search --explain --dry-run --query JSON` | Plan only; do not execute. |
| `octocode search --json` | Return the typed result envelope. |
| `octocode search --compact` | Lowest-token human/agent view. |

Exit codes follow the CLI convention: `0` ok, `2` bad input, `3` not found,
`4` auth, `5` tool error, `7` rate limited.

**Always-true rules**

- Canonical OQL is a typed object, never a string DSL. Sugar is accepted at the
  edge and normalized away; `--explain` shows the canonical query that ran.
- Every `where` leaf is a discriminated predicate with `kind`.
- No predicate is ever silently dropped. If a predicate cannot be evaluated, you
  get a diagnostic, not a weaker query.
- OQL never materializes, runs LSP, or fetches silently — those are explicit.
- Provider zero-results are proof of absence only when every predicate was
  evaluated by the provider over a complete universe.
- OQL never mutates files.

---

## 3. Targets

Choose exactly one `target`. Active targets execute today.

| Target | Result | Backing runner(s) |
|---|---|---|
| `code` | code match occurrences | `localSearchCode`, `ghSearchCode` (+ materialized local) |
| `content` | file/range content blocks | `localGetFileContent`, `ghGetFileContent` |
| `structure` | directory/repo tree entries | `localViewStructure`, `ghViewRepoStructure` |
| `files` | file/directory entries | `localFindFiles` (+ `localSearchCode` for content predicates) |
| `semantics` | LSP symbols / locations / calls | `lspGetSemantics` (remote routes through clone) |
| `repositories` | repository rows | `ghSearchRepos` |
| `packages` | npm package rows | `npmSearch` |
| `pullRequests` | PR rows / detail | `ghHistoryResearch` |
| `commits` | commit rows / history | `ghHistoryResearch type:"commits"` |
| `artifacts` | binary/archive inspection | `localBinaryInspect` |
| `diff` | PR patch or two-ref file diff | `ghHistoryResearch` / `ghGetFileContent` + local line diff |
| `research` | heuristic repo research packet | OQL research analyzer |
| `materialize` | a stable local checkpoint | `ghCloneRepo` + cache |

Reserved targets return `unsupportedTarget` until proof/dry-run engines exist:

| Target | Why reserved |
|---|---|
| `fixes` | No dry-run codemod engine yet. |
| `dataflow` | No trace-backed flow engine yet. |

---

## 4. Canonical query shape

```ts
type OqlActiveTarget =
  | "code" | "content" | "structure" | "files"
  | "semantics" | "repositories" | "packages"
  | "pullRequests" | "commits" | "artifacts" | "diff"
  | "research" | "materialize"

type OqlReservedTarget = "fixes" | "dataflow"

interface OqlQuery {
  schema: "oql"
  id?: string
  target: OqlActiveTarget
  // Required for every target except registry/provider discovery
  // (`packages`, `repositories`), which default their corpus.
  from?: QuerySource
  scope?: QueryScope
  where?: Predicate
  materialize?: MaterializePolicy
  fetch?: FetchInstructions
  select?: string[]
  view?: "discovery" | "paginated" | "detailed"
  controls?: QueryControls
  limit?: number
  page?: number
  itemsPerPage?: number
  params?: Record<string, unknown>   // target-specific operation bag
  explain?: boolean
}

interface OqlBatch {
  schema: "oql"
  id?: string
  queries: OqlQuery[]                // 1–5
  combine?: "independent" | "merge"
  limit?: number
  page?: number
  itemsPerPage?: number
  explain?: boolean
}
```

**Schema rules**

- `schema:"oql"` rejects unknown canonical fields with `unknownField`.
- Raw input may omit `schema`; normalization inserts it.
- Raw input may omit `target` — it is inferred from sugar (`text`/`regex`/
  `pattern`/`rule` → `code`, `fetch.content`/`minify` → `content`,
  `fetch.tree` → `structure`).
- A reserved target in raw input is preserved so normalization can return
  `unsupportedTarget`; canonical execution accepts active targets only.

**Per-target shape (matching/fetch targets)**

| Target | Required | Optional | Never uses |
|---|---|---|---|
| `code` | `from`, `where` | `scope`, `materialize`, `select`, `view`, `controls`, bounds | `fetch` (only as emitted continuation) |
| `content` | `from` | `fetch.content`, `select`, `view`, bounds | `where` |
| `structure` | `from` | `scope.path`, `fetch.tree`, `select`, `view`, bounds | `where` |
| `files` | `from` | `scope`, `where` (field/content/boolean), `select`, `view`, bounds | `fetch.content` |

Operation targets (`semantics`, `repositories`, `packages`, `pullRequests`,
`commits`, `artifacts`, `diff`, `research`, `materialize`) use `params` instead
of `where`/`fetch` — see [§8](#8-params-per-target).

**Evaluation units** — every predicate is evaluated against a target-specific
unit; the planner preserves it through pushdown / materialization / filtering:

| Target | Unit | `where` omitted |
|---|---|---|
| `code` | one code match occurrence | invalid — `code` needs a real predicate |
| `content` | one content block/file read | allowed (fetch only) |
| `structure` | one tree entry | allowed (tree only) |
| `files` | one file/dir entry | allowed (list files in scope) |

On `target:"files"`, a content predicate means "file contains ≥1 match"; `not`
over it means "file contains none". Negative file queries need a complete
candidate universe — local scope provides it; a GitHub source requires
materialization or returns a diagnostic.

---

## 5. Source (`from`)

```ts
type QuerySource =
  | { kind: "local"; path: string }
  | { kind: "github"; repo?: string; owner?: string; ref?: string }
  | { kind: "materialized"; localPath: string; source?: QuerySource }
  | { kind: "npm" }
```

| Field | Required when | Meaning |
|---|---|---|
| `path` | local | file or directory root |
| `repo` | GitHub (canonical) | `"owner/name"` repository id |
| `owner` | GitHub (raw input) | user/org scope for provider search |
| `ref` | optional GitHub | branch, tag, or commit |
| `localPath` | materialized | local path returned by materialization |
| `source` | optional materialized | original provider source |
| `kind:"npm"` | package discovery | registry corpus (default for `packages`) |

**Rules**

- `from` identifies the corpus; repository subpaths go in `scope.path`.
- Canonical GitHub identity is `repo:"owner/name"`. Raw `{owner,repo}` (with a
  bare repo name) normalizes to `repo:"owner/name"`.
- `owner` without `repo` scopes provider search to one owner.
- `from:{kind:"github"}` with no owner/repo means provider-wide search — valid
  only for provider-search targets (`code`, `repositories`) and never
  materialized.
- `content`, `structure`, and `materialize` over GitHub require a concrete
  `repo:"owner/name"` (a provider-wide/owner-only source cannot read a specific
  tree → `invalidQuery` with a repair).
- `from` may be omitted for `packages` (defaults to `{kind:"npm"}`) and
  `repositories` (defaults to `{kind:"github"}`).
- Local paths pass existing path validation; secret sanitization is applied on
  output. Both survive every adapter.

---

## 6. Scope

```ts
interface QueryScope {
  path?: string | string[]
  language?: string | string[]
  include?: string[]
  exclude?: string[]
  excludeDir?: string[]
  hidden?: boolean
  noIgnore?: boolean
  maxDepth?: number
}
```

| Field | Meaning | Honored by |
|---|---|---|
| `path` | traversal root / provider path prefix | all targets (first entry; sparse path when materializing) |
| `language` | canonical language filter | local/GitHub code search, `files` (→ name glob) |
| `include` | include globs | local code search |
| `exclude` | exclude globs | local code search |
| `excludeDir` | directory names to skip | local code search, `files` |
| `hidden` | include hidden files | local code search, `structure` |
| `noIgnore` | ignore `.gitignore`/ignore files | local code search |
| `maxDepth` | traversal depth | `files`, `structure` |

**Rules**

- `scope.path` narrows the traversal root or provider prefix. It is **not** the
  proof language for path matching. For exact path intent use
  `where:{kind:"field",field:"path",op:"glob"|"regex"}`.
- Provider path qualifiers are prefix filters; treat them as approximate unless
  a local/materialized residual check proves them.
- A scope field a backend cannot honor becomes a diagnostic (or routes to a
  local/materialized lane), never a silent drop.
- Structural search currently honors `include`, `excludeDir`, language-derived
  includes, and `maxFiles`; other scope fields must be residual or diagnostic
  before they affect proof.

**Path sugar resolution** (raw input):

- `repo` present + top-level `path` → `path` means `scope.path`.
- no `repo`/`from` + top-level `path` → `path` means `from:{kind:"local",path}`.
- explicit `from` + top-level `path` → `path` means `scope.path`.
- both `scope.path` and top-level `path` → `ambiguousSugar`.

---

## 7. Predicates (`where`)

`where` is a discriminated union. Every leaf has a `kind`. Used by `code` and
`files` only.

```ts
type Predicate =
  | { kind: "all"; id?: string; of: Predicate[] }
  | { kind: "any"; id?: string; of: Predicate[] }
  | { kind: "not"; id?: string; predicate: Predicate }
  | TextPredicate | RegexPredicate | StructuralPredicate | FieldPredicate
```

### Text

```ts
interface TextPredicate {
  id?: string
  kind: "text"
  value: string
  case?: "smart" | "sensitive" | "insensitive"   // default "smart"
  wholeWord?: boolean
}
```

`text` is literal text. Local compilation sets `fixedString:true`. `smart` case
follows local ripgrep behavior. Provider text search may not honor exact local
case — unsupported parts appear in diagnostics.

### Regex

```ts
interface RegexPredicate {
  id?: string
  kind: "regex"
  value: string
  dialect?: "rust" | "pcre2" | "provider"   // default "rust"
  case?: "smart" | "sensitive" | "insensitive"
  wholeWord?: boolean
  multiline?: boolean
  dotAll?: boolean
}
```

- Default local dialect is `rust`.
- `pcre2` (lookaround, backreferences) requires local or materialized execution.
- `provider` regex over GitHub is approximate; the planner emits
  `providerSemanticsApproximate` and offers materialization for exact proof.

### Structural (AST)

```ts
interface StructuralPredicate {
  id?: string
  kind: "structural"
  lang: string                 // required parser language
  pattern?: string             // code-shaped pattern  (exactly one of)
  rule?: StructuralRule        // JSON relational rule  (pattern | rule)
}

interface StructuralRule {
  pattern?: string
  kind?: string                // AST node kind
  inside?: StructuralRule      // ancestor/containing
  has?: StructuralRule         // descendant/subtree
  not?: StructuralRule         // negated nested rule
  all?: StructuralRule[]
  any?: StructuralRule[]
  stopBy?: "end"               // relational traversal boundary
}
```

Active structural features:

| Feature | Notes |
|---|---|
| `pattern` | code-shaped tree-sitter pattern |
| `$X` | capture one AST node |
| `$$$ARGS` / `$$$NAME` | capture a node list (variadic) |
| `rule.pattern` | nested code-shaped pattern |
| `rule.kind` | AST node-kind constraint |
| `rule.inside` / `rule.has` | containing / descendant relations |
| `rule.not` | structural negation inside a rule |
| `rule.all` / `rule.any` | rule composition |
| `rule.stopBy:"end"` | required for bounded relational intent |

Deferred (currently `unsupportedPredicate`): `precedes`, `follows`, `field`,
`regex`, `nthChild`, `range`, `matches`; reusable named refs; metavariable
constraints / focus ranges; rule ids/messages/severities/tests; `fix`/
`transform`. OQL structural rules are Octocode tree-sitter rules — not full
ast-grep or Semgrep compatibility.

Rules:

- Exactly one of `pattern` or `rule`; `lang` is required.
- Patterns must parse as complete source nodes — a TS class needs
  `class $NAME { $$$BODY }`, not `class $NAME`.
- Structural search is **local-only**. GitHub structural queries need
  `materialize.mode:"auto"` or `"required"`.
- The compiler serializes `rule` to the engine's YAML rule form; author the JSON
  object — do not hand-write YAML.

### Field

```ts
interface FieldPredicate {
  id?: string
  kind: "field"
  field: "path" | "basename" | "extension" | "size" | "modified" | "entryType"
  op: "=" | "!=" | "in" | "exists" | "glob" | "regex" | ">" | ">=" | "<" | "<=" | "within"
  value?: unknown            // omitted only for op:"exists"
}
```

| Operator | Meaning | Value shape |
|---|---|---|
| `=` / `!=` | equality | scalar matching field type |
| `in` | one of | non-empty array |
| `exists` | field present | (none) |
| `glob` | glob compare (path-like) | glob string, `/`-separated |
| `regex` | regex compare (path-like) | regex string |
| `>` `>=` `<` `<=` | numeric/timestamp compare | number, size, or ISO timestamp |
| `within` | range/window | `{from?,to?}` or a duration like `"7d"` |

Value conventions: size `/^[0-9]+(b|k|m|g)$/` (base 1024, lowercase); duration
`/^[0-9]+(m|h|d|w)$/`; timestamps ISO 8601; `extension` normalizes without a
leading dot; `entryType` is `"file"` or `"directory"`. Type mismatches →
`fieldTypeMismatch`.

Implemented mapping (local `files`/`structure`):

| Field / op | Backend |
|---|---|
| `path`/`basename` `regex` | `localFindFiles regex` |
| `path`/`basename` `glob`/`=`/`in` | `localFindFiles names` |
| `extension` (any) | `localFindFiles names:["*.<ext>"]` |
| `size` `>`/`>=` | `sizeGreater` |
| `size` `<`/`<=` | `sizeLess` |
| `modified` `within` | `modifiedWithin` (relative window only) |
| `entryType` | `f` / `d` |

`modified` with absolute `>`/`<` → `unsupportedPredicate` (findFiles has no
absolute-date filter). Negated field predicates over findFiles are best-effort
(`residualNotExact` warning). `path`/`basename`/`extension` can route to GitHub
as provider candidates; exact glob/regex proof routes to materialization.

### Booleans and sugar

`all` (every child), `any` (≥1 child), `not` (exactly one child). Empty
`all.of`/`any.of` is invalid.

Accepted sugar (normalized before planning):

| Sugar | Canonical |
|---|---|
| `and` | `all` |
| `or` | `any` |
| `noneOf:[A,B]` | `not(any(A,B))` |
| `xor:[A,B]` | `any(all(A,not B), all(not A,B))` (binary only) |
| `oneOf:[A,B,…]` | exactly-one expansion |
| `invert:true` | wrap predicate in `not` |

`xor` is binary — use `oneOf` for multi-way exclusive matching. Boolean
expansion is bounded by `controls.budget.maxBooleanExpansion` (default 64); an
over-budget `oneOf`/`xor` fails with `budgetExhausted`.

**Boolean execution**

- `target:"files"` — file-level set algebra: `all`=intersection, `any`=union,
  `not`=universe−set. Local scope is the complete universe, so negation is exact
  (no `negativeUniverseRequired`).
- `target:"code"` — per-leaf match rows combined by file-set algebra: `all`
  shows occurrences from files matching every branch; `any` is the union of
  occurrences; `not`/`field` branches contribute a file-set constraint, not
  occurrences. A single leaf (or `not(leaf)`) is one `localSearchCode` call
  (`not(leaf)` → `invertMatch`). Over a GitHub source, a boolean routes through
  bounded materialization, then runs locally.

Safe routing for booleans over a provider source:

| Shape | Rule |
|---|---|
| `all(PUSHDOWN, RESIDUAL)` | push supported, residual-filter the rest |
| `any(PUSHDOWN, RESIDUAL)` | needs union coverage; else materialize or fail |
| `not(P)` | exact only with the full evaluation universe |
| `xor(A,B)` | exact only when both branches and negations are exact over one universe |

---

## 8. `params` per target

Operation targets carry a typed `params` bag. OQL validates the documented
common fields early (a type mistake like `prNumber:"abc"` fails with
`invalidQuery params.prNumber: …`); the backing tool validates the rest. Run
`octocode tools <name> --scheme` for the exhaustive schema.

### `semantics` → `lspGetSemantics`

```ts
{
  type?: "definition" | "references" | "callers" | "callees" | "callHierarchy"
       | "hover" | "documentSymbols" | "typeDefinition" | "implementation"
  uri?: string                // for remote, used as the sparse path to clone
  symbolName?: string
  lineHint?: number           // 1-based
  orderHint?: number
  includeDeclaration?: boolean
  depth?: number              // 0–20
  groupByFile?: boolean
  workspaceRoot?: string
  format?: "structured" | "compact"
  page?: number
  itemsPerPage?: number
}
```

Semantic queries need a real anchor (a `uri`/local path + line, not a name
alone). Remote semantics clone the file first, then run LSP locally — clone
provenance is preserved. Provider `symbol:` search can discover definitions but
is not reference/type/implementation/call-hierarchy proof.

### `repositories` → `ghSearchRepos`

```ts
{
  keywords?: string[]
  topicsToSearch?: string[]
  language?: string
  owner?: string
  stars?: string | number
  size?: string
  updated?: string
  license?: string
  visibility?: "public" | "private"
  archived?: boolean
  sort?: "stars" | "forks" | "help-wanted-issues" | "updated" | "best-match"
  concise?: boolean
  page?: number
}
```

### `packages` → `npmSearch`

```ts
{ packageName?: string; keywords?: string[]; mode?: "lean" | "full"; page?: number }
```

### `pullRequests` → `ghHistoryResearch`

```ts
{
  prNumber?: number
  state?: "open" | "closed" | "merged"
  author?: string
  label?: string | string[]
  keywordsToSearch?: string[]
  head?: string
  base?: string
  reviewMode?: string
  filePage?: number; commentPage?: number; commitPage?: number
  charOffset?: number; charLength?: number
  minify?: "none" | "standard"
  limit?: number; page?: number
}
```

### `commits` → `ghHistoryResearch type:"commits"`

```ts
{ path?: string; branch?: string; since?: string; until?: string;
  includeDiff?: boolean; limit?: number; page?: number }
```

### `artifacts` → `localBinaryInspect`

```ts
{
  mode?: "inspect" | "list" | "extract" | "decompress" | "strings" | "unpack"
  archiveFile?: string
  entryPageNumber?: number; entriesPerPage?: number
  minLength?: number          // 1–128 (strings)
  scanOffset?: number         // strings scan cursor
  charOffset?: number; charLength?: number
  matchString?: string
  verbose?: boolean
}
```

`from` must be a local file path. `extract`/`decompress`/`unpack` produce a
derived `localPath` that the result row exposes as `next.structure`/`next.files`.
The `strings` scan cursor surfaces as `next.artifactStrings`.

### `diff` → two typed lanes

```ts
// PR patch lane:
{ prNumber: number; files?: string[] }
// direct two-ref file lane:
{ baseRef: string; headRef: string; path: string }
```

The PR-patch lane fetches patches via `ghHistoryResearch`. The direct-file lane
reads the file at both refs (`ghGetFileContent`) and computes a pure local line
diff. A request that fits neither shape returns `invalidQuery` with a repair —
there is no silent fallthrough.

### `research` → OQL research analyzer

```ts
{
  goal?: string
  intent?: "general" | "reachability" | "dependencies" | "symbols"
  facets?: string[]
  mode?: "plan" | "analyze"
  maxFiles?: number
}
```

See [§16. Research flows](#16-research-flows).

### `materialize`

No `params` and no `where`. Needs `from:{kind:"github",repo:"owner/name"}` (plus
`scope.path` to bound the subtree) or an already-materialized `from`. Returns a
checkpoint row — see [§9](#9-materialization).

---

## 9. Materialization

```ts
interface MaterializePolicy {
  mode: "never" | "auto" | "required"
  strategy?: "file" | "tree" | "subtree" | "repo"   // default "subtree"
  allowFullRepo?: boolean
  forceRefresh?: boolean
}
```

| Field | Meaning |
|---|---|
| `mode:"never"` | provider-only; local-only proof → `requiresMaterialization` |
| `mode:"auto"` | planner may materialize a bounded source when needed |
| `mode:"required"` | planner must materialize first or fail |
| `strategy:"file"` | one file |
| `strategy:"tree"` | tree info |
| `strategy:"subtree"` | bounded subtree clone (sparse checkout of `scope.path`) |
| `strategy:"repo"` | full repo — only with `allowFullRepo:true` |
| `forceRefresh` | bypass cached clone |

**Defaults:** local sources need no materialization. GitHub sources default to
`mode:"never"`, except a local-only predicate (structural / PCRE2) with no
explicit policy defaults to `{mode:"auto",strategy:"subtree"}`.

**Bounds (enforced):**

- A subtree clone requires `scope.path` (or `strategy:"file"`), otherwise
  `materializationNotAllowed` blocks execution — OQL refuses to clone an
  unbounded scope.
- `strategy:"repo"` requires `allowFullRepo:true`; otherwise repair to
  `subtree` with a concrete `scope.path`.
- Broad org/user/global scopes are never materialized.

**Remote-as-local flow** — one query can: (1) use provider search/tree/content
to bound candidates, (2) materialize the bounded file/subtree, (3) run local
proof tools (ripgrep, PCRE2, structural AST, exact content, symbol outline),
(4) return one envelope with per-step provenance and continuations. This is the
OQL form of `grep --repo`: remote input, local proof. Provider zero-results are
not proof when the query asked for a local-only predicate.

**`target:"materialize"`** is addressable materialization: clone/cache once and
return a `recordType:"materialized"` checkpoint row (`localPath`, `repoRoot`,
`ref`, `cache`, `complete`) with `next.structure`/`next.files`. `complete` is
`true` only for a full-repo clone; a bounded subtree is materialized-but-partial.
Backing tool: `ghCloneRepo` (`owner`, `repo`, `branch`, `sparsePath`,
`forceRefresh`).

---

## 10. Fetch

```ts
interface FetchInstructions {
  content?: {
    range?: { startLine?: number; endLine?: number; contextLines?: number }
    match?: { text: string; regex?: boolean; caseSensitive?: boolean }
    contentView?: "exact" | "compact" | "symbols"   // default "compact"
    charOffset?: number
    charLength?: number
    fullContent?: boolean
  }
  tree?: { maxDepth?: number; includeSizes?: boolean }
}
```

Content view mapping:

| View | Backing minify | Use |
|---|---|---|
| `exact` | `none` | quotes, patches, diffs |
| `compact` | `standard` | normal reading |
| `symbols` | `symbols` | cheap signature/symbol outline |

`symbols` is syntactic, not LSP proof. If symbol extraction is unsupported for a
language, OQL returns `signatureUnsupported`/`partialResult` plus a content
continuation. A `symbols` view is not available for PR/commit/diff content and
returns `signatureUnsupported`.

Both the local and GitHub content lanes forward line ranges, `contextLines`,
`match.text`/`match.regex`/`match.caseSensitive`, char windows, and
`fullContent`.

---

## 11. Controls

```ts
interface QueryControls {
  search?: {
    countLinesPerFile?: boolean
    countMatchesPerFile?: boolean
    onlyMatching?: boolean
    unique?: boolean            // requires onlyMatching
    countUnique?: boolean       // requires onlyMatching
    matchWindow?: number
    matchContentLength?: number
    maxMatchesPerFile?: number
    matchPage?: number
    sort?: "relevance" | "matchCount" | "path" | "modified" | "accessed" | "created"
    sortReverse?: boolean
    rankingProfile?: string
    debugRanking?: boolean
  }
  budget?: {
    maxFiles?: number
    maxCandidates?: number
    maxBytes?: number
    maxMaterializedBytes?: number
    maxPlanNodes?: number
    maxBooleanExpansion?: number
    timeoutMs?: number
  }
}
```

`controls` affects cost, pagination, snippets, ranking, and debug output — never
what logically matches. `onlyMatching` is the safe way to enumerate values from
minified one-line files. `matchPage` pages matches inside one file; top-level
`page` pages result rows.

**Budget enforcement status:**

| Budget | Status |
|---|---|
| `maxFiles` | enforced (local code search) |
| `maxPlanNodes` | enforced (explain-plan node cap, default 128) |
| `maxBooleanExpansion` | enforced (boolean sugar expansion, default 64) |
| `maxCandidates`, `maxBytes`, `maxMaterializedBytes`, `timeoutMs` | accepted; enforced only where the backend supports it (no hard global cap yet) |

`rankingProfile` and `debugRanking` forward to local code search.

---

## 12. View, select, bounds

**`view`** controls output density, not matching:

- `discovery` — paths only (code → files-only search). Pair with
  `select:["path","next.fetch"]`.
- `paginated` — default.
- `detailed` — code adds context lines (3).

**`select`** projects which fields and continuations appear on each row. It is
projection only — it never changes the result domain or triggers a hidden
fetch. Identity fields (`kind`, `source`, `recordType`, `id`) always survive.
Recognized field tokens: `path`, `line`, `endLine`, `column`, `snippet`,
`content`, `contentView`, `range`, `metavars`, `metavarRanges`, `size`,
`modified`, `entryType`, `depth`, `children`, `data`; continuation tokens
`next` (all) or `next.<domain>`; envelope tokens `pagination`, `diagnostics`,
`provenance`, `evidence`, `repo`, `localPath`. Unknown tokens are ignored with a
non-blocking `unknownField` diagnostic.

**`limit` / `page` / `itemsPerPage`** bound the primary result-row domain.
`limit` caps the logical row count; `page`/`itemsPerPage` page rows. Match pages
(`controls.search.matchPage`) and content char windows
(`fetch.content.charOffset`) are separate domains — see [§15](#15-result-envelope).

---

## 13. Defaults

`octocode search --explain` surfaces every applied default.

| Field | Default |
|---|---|
| `schema` | `oql` |
| `view` | `paginated` |
| `page` | `1` |
| `itemsPerPage` | `25` (unless a target cap is lower) |
| `materialize.mode` (GitHub) | `never` (or `auto` for a local-only predicate) |
| `text.case` | `smart` |
| `regex.dialect` (local) | `rust` |
| `regex.case` | `smart` |
| `fetch.content.contentView` | `compact` |
| `fetch.content.charLength` | `20000` (when paginating content) |
| `controls.search.matchContentLength` | `500` |
| `controls.budget.maxPlanNodes` | `128` |
| `controls.budget.maxBooleanExpansion` | `64` |
| normal code context | `2` lines |
| detailed code context | `3` lines |
| local search sort | `relevance` |
| local ranking profile | `auto` |

---

## 14. Normalization & sugar

Sugar is accepted only with a deterministic rewrite; ambiguous sugar fails with
`ambiguousSugar`; canonical `--explain` output contains no shorthand.

| Sugar | Canonical |
|---|---|
| top-level `repo` | `from:{kind:"github",repo}` |
| top-level `owner` + `repo` | `from:{kind:"github",repo:"owner/name"}` |
| top-level local `path` (no repo) | `from:{kind:"local",path}` |
| GitHub `path` | `scope.path` |
| `--type` / `langType` | `scope.language` (or structural `lang` by context) |
| `text` | `where:{kind:"text",value}` |
| `regex` | `where:{kind:"regex",value}` |
| `pattern` + `lang` | `where:{kind:"structural",lang,pattern}` |
| `rule` + `lang` | `where:{kind:"structural",lang,rule}` |
| `minify` | `fetch.content.contentView` |
| `and` / `or` | `all` / `any` |
| `xor` | `any(all(A,not B),all(not A,B))` |
| `noneOf` / `oneOf` | `not(any(...))` / exactly-one expansion |
| `invert:true` | wrap in `not` |
| `filesOnly:true` | `view:"discovery"` + `select:["path","next.fetch"]` |
| `filesWithoutMatch` | `target:"files"` + `where:{kind:"not",predicate}` |

Example — sugar in:

```jsonc
{ "repo": "facebook/react", "path": "packages/react",
  "pattern": "useEffect($$$ARGS)", "lang": "js" }
```

Canonical out:

```jsonc
{
  "schema": "oql", "target": "code",
  "from": { "kind": "github", "repo": "facebook/react" },
  "scope": { "path": "packages/react" },
  "where": { "kind": "structural", "lang": "js", "pattern": "useEffect($$$ARGS)" },
  "materialize": { "mode": "auto", "strategy": "subtree" },
  "view": "paginated"
}
```

---

## 15. Planner

| Mode | Meaning |
|---|---|
| `PUSHDOWN` | backend evaluates the predicate directly |
| `RESIDUAL` | fetch bounded candidates and filter locally |
| `ROUTE` | move to another lane (usually materialization) |
| `UNSUPPORTED` | fail with diagnostics + repair |

Invariant: `pushed + residual + routed + unsupported == all predicate nodes`.
Every node (boolean and leaf) is recorded and routed; stable predicate IDs come
from node position or a user `id`.

`octocode search --explain` returns:

```ts
interface OqlExplainPlan {
  input: unknown
  normalized: OqlCanonicalInput
  defaults: Record<string, unknown>
  nodes: Array<{ predicateId: string; path: string;
    route: "PUSHDOWN" | "RESIDUAL" | "ROUTE" | "UNSUPPORTED";
    backend?: string; reason: string }>
  backendCalls: Array<{ backend: string; source?: QuerySource;
    operation: string; exact: boolean }>
  materialization?: MaterializePolicy & { required: boolean; reason: string }
  budgets: QueryControls["budget"]
  truncated?: boolean
  diagnostics: OqlDiagnostic[]
  next?: Record<string, OqlContinuation>
}
```

Plan-node count is bounded by `maxPlanNodes`; an over-budget plan sets
`truncated:true` + `planTruncated` but never changes execution semantics. Cheap
filters are ordered before expensive fetch/materialization.

**Backend mapping (summary):**

| Canonical query | Compilation |
|---|---|
| local `code` + `text` | `localSearchCode keywords + fixedString:true` |
| local `code` + `regex.rust` | `localSearchCode keywords` |
| local `code` + `regex.pcre2` | `localSearchCode perlRegex:true` |
| local `code` + `structural` | `localSearchCode mode:"structural"` |
| local `code` + boolean | per-leaf `localSearchCode` + file-set algebra |
| local `content` / `structure` / `files` | `localGetFileContent` / `localViewStructure` / `localFindFiles` |
| GitHub `code` (provider-capable) | `ghSearchCode` |
| GitHub `content` / `structure` | `ghGetFileContent` / `ghViewRepoStructure` |
| GitHub `files` + positive text/regex | `ghSearchCode` (path-level file set, approximate) |
| GitHub local-only predicate, boolean, negation, `files`-by-attribute + `materialize:auto` | `ghCloneRepo` then local tool |

---

## 16. Result envelope

```ts
interface OqlResultEnvelope {
  queryId?: string
  queryIndex?: number
  results: OqlResultRow[]
  pagination?: Pagination
  next?: Record<string, OqlContinuation>
  diagnostics: OqlDiagnostic[]
  provenance: OqlProvenance[]
  evidence: { answerReady: boolean; complete: boolean;
    kind: "proof" | "partial" | "candidate" | "unsupported" }
  plan?: OqlExplainPlan          // only with explain:true / --explain
}
```

### Rows

```ts
type OqlResultRow =
  | OqlCodeResultRow | OqlFileResultRow | OqlTreeResultRow
  | OqlContentResultRow | OqlRecordResultRow

interface OqlCodeResultRow {
  kind: "code"; source: QuerySource; path: string
  line?: number                  // omitted for provider path-level matches
  endLine?: number; column?: number; snippet?: string
  metavars?: Record<string, string[]>        // structural captures
  metavarRanges?: Record<string, { text: string; line: number; column: number;
    endLine: number; endColumn: number }[]>
  next?: Record<string, OqlContinuation>
}

interface OqlFileResultRow {
  kind: "file"; source: QuerySource; path: string
  entryType: "file" | "directory"; size?: number; modified?: string
  next?: Record<string, OqlContinuation>
}

interface OqlTreeResultRow {
  kind: "tree"; source: QuerySource; path: string
  entryType: "file" | "directory"; depth: number; size?: number
  children?: OqlTreeResultRow[]; next?: Record<string, OqlContinuation>
}

interface OqlContentResultRow {
  kind: "content"; source: QuerySource; path: string; content: string
  range?: { startLine?: number; endLine?: number; charOffset?: number; charLength?: number }
  contentView: "exact" | "compact" | "symbols"
  next?: Record<string, OqlContinuation>
}

// All operation targets return record rows.
interface OqlRecordResultRow {
  kind: "record"
  recordType: "semantics" | "repository" | "package" | "pullRequest" | "commit"
            | "artifact" | "diff" | "research" | "materialized"
  id?: string                    // citeable identity (repo, name@version, #PR, sha, path, uri)
  source?: QuerySource
  data: Record<string, unknown>  // backing-tool payload; documented per recordType in oql/types.ts
  next?: Record<string, OqlContinuation>
}
```

Documented `data` field shapes per `recordType` (and typed row aliases like
`OqlRepositoryRow`, `OqlPullRequestRow`) live in
`packages/octocode-tools-core/src/oql/types.ts`. All fields are
backend-dependent and never fabricated.

### Pagination

```ts
interface Pagination {
  currentPage?: number; totalPages?: number; itemsPerPage?: number
  totalItems?: number; hasMore: boolean; next?: OqlContinuation
}
```

Pagination domains stay separate; only one primary domain is reflected in
top-level `pagination`:

| Domain | Fields | Continuation |
|---|---|---|
| result rows | `page`, `itemsPerPage`, `limit` | `next.page` |
| matches within a file | `controls.search.matchPage`, `maxMatchesPerFile` | `next.matchPage` |
| content windows | `fetch.content.charOffset`, `charLength` | `next.charRange` |
| artifact entries | artifact page fields | `next.artifactEntries` |
| binary string scan | `params.scanOffset` | `next.artifactStrings` |

### Provenance & evidence

```ts
interface OqlProvenance {
  backend: string; source?: QuerySource
  predicateIds?: string[]; pushed?: string[]; residual?: string[]; routed?: string[]
  materializedPath?: string; cache?: "hit" | "miss" | "refresh" | "stale"
}
```

| Evidence kind | Meaning |
|---|---|
| `proof` | every required predicate evaluated exactly over the required universe |
| `partial` | pages, candidates, files, or residual checks remain |
| `candidate` | useful lead; at least one predicate/filter is approximate |
| `unsupported` | the requested semantics could not be executed |

`answerReady:true` requires `kind:"proof"` and `complete:true` (unless the user
asked only for candidates). Diagnostics that block proof set `blocksAnswer:true`.
A result with executable continuations is usually `partial` unless the
continuation is optional enrichment.

### Continuations

```ts
interface OqlContinuation {
  query: OqlCanonicalInput       // runnable as-is
  baseQueryId?: string; queryIndex?: number
  why: string
  confidence: "exact" | "heuristic"
}
```

Continuations are full, executable OQL queries — follow them instead of
computing offsets, pages, or paths. Names in use:

`next.page`, `next.matchPage`, `next.charRange`, `next.fetch`, `next.structure`,
`next.files`, `next.search`, `next.materialize`, `next.semantic`,
`next.packageSource`, `next.pullRequestPage`, `next.commitPage`,
`next.artifactEntries`, `next.artifactStrings`, `next.diff`.

Built today: code rows emit `next.fetch` (+ `next.semantic` on local);
content rows emit `next.charRange`; artifact and materialized rows emit
`next.structure`/`next.files` (and `next.artifactStrings` for string scans);
semantics rows emit `next.fetch`; the envelope emits `next.page`/`next.matchPage`.

---

## 17. Diagnostics

```ts
interface OqlDiagnostic {
  code: DiagnosticCode
  severity: "info" | "warning" | "error"
  queryPath?: string; predicateId?: string; backend?: string
  message: string
  blocksAnswer: boolean
  repair?: { message: string; suggestedQuery?: OqlSearchInput }
  continuation?: OqlContinuation
}
```

| Code | Meaning |
|---|---|
| `invalidQuery` | schema or normalization failed |
| `ambiguousSugar` | sugar has more than one canonical meaning |
| `unknownField` | field/select token outside OQL |
| `unsupportedTarget` | reserved or unknown target |
| `unsupportedPredicate` | backend cannot evaluate the predicate |
| `unsupportedBoolean` | boolean shape not evaluable as requested |
| `unsupportedScope` | backend cannot honor scope exactly |
| `negativeUniverseRequired` | negation needs a complete candidate universe |
| `residualNotExact` | residual filtering cannot preserve proof strength |
| `fieldTypeMismatch` | field value/operator type mismatch |
| `requiresMaterialization` | local-only proof requested on a provider source |
| `materializationNotAllowed` | needs materialization but mode is `never` / unbounded |
| `materializationFailed` | clone/fetch/cache failed |
| `providerUnindexed` | provider search may be incomplete — verify before "absent" |
| `providerSemanticsApproximate` | provider qualifier is candidate-grade |
| `partialResult` | more result/match pages exist |
| `contentTruncated` | content cut by a char/window budget |
| `matchTruncated` | per-file matches were capped |
| `planTruncated` | explain-plan nodes were capped |
| `budgetExhausted` | stopped at an explicit budget |
| `parserFailed` | structural parser failed |
| `partialParse` | some files parsed; parser errors may hide matches |
| `signatureUnsupported` | symbol/signature view unavailable |
| `lspUnavailable` | semantic op needs LSP but no server/capability |
| `staleCache` | cached materialization may be stale |
| `sanitized` | secret/path sanitization changed output |
| `rateLimited` | provider rate limit blocked full execution |
| `zeroMatches` | query ran completely and matched nothing |

`zeroMatches` is "ran and found nothing" — not the same as `providerUnindexed`
("provider may not have indexed this; verify").

---

## 18. Batch

```ts
interface OqlBatch {
  schema: "oql"; id?: string
  queries: OqlQuery[]            // 1–5
  combine?: "independent" | "merge"
  limit?: number; page?: number; itemsPerPage?: number; explain?: boolean
}
```

- `combine:"independent"` (default) returns one envelope per query.
- `combine:"merge"` merges rows only when every child shares the same result
  kind; rows dedup by stable identity (`source:path:line`). Mixed kinds →
  `invalidQuery` with a repair to use `independent`.
- Query order is stable; a failed query does not erase sibling results; each
  child keeps its own diagnostics, provenance, evidence, pagination, and
  continuations.

```ts
interface OqlBatchResultEnvelope {
  batchId?: string
  mode: "independent" | "merge"
  children: Array<{ queryId: string; queryIndex: number; envelope: OqlResultEnvelope }>
  merged?: OqlResultEnvelope
  diagnostics: OqlDiagnostic[]
}
```

---

## 19. Research flows

`target:"research"` runs an internal **heuristic** analyzer over a complete
local file universe. It is for orientation and candidate discovery — *not*
proof. Use a local or materialized source; a GitHub source must materialize a
bounded corpus first (the adapter returns `requiresMaterialization` with a
repair otherwise).

**Intent** (auto-inferred from `goal`/`facets`, or set explicitly):

| Intent | Question |
|---|---|
| `reachability` | What looks dead / unreachable from entrypoints? |
| `dependencies` | What's unlisted / unused / duplicated in manifests? |
| `symbols` | Which exports are referenced / unused? |
| `general` | Structure + symbols + dependency overview. |

**Mode:** `plan` returns only the staged flow (no file scan); `analyze` scans and
returns findings.

**What it actually computes** (all heuristic):

1. **Inventory** — walk files (default-excluding `.git`, `node_modules`, `dist`,
   `out`, `coverage`, `target`, `.next`, `.turbo`, `.yarn`, `.claude`,
   `.context`, `.cursor`), find `package.json` manifests.
2. **Manifest graph** — entrypoints (`main`/`module`/`types`/`bin`/`exports` +
   `src/index.*` fallbacks), declared deps, workspace package names.
3. **Import graph & reachability** — resolve relative imports between source
   files; reachable = transitive closure from entrypoints.
4. **Dependency audit** — compare import specifiers with manifests → unlisted /
   unused / duplicate dependencies.
5. **Export symbols** — regex-scan `export` declarations; score references by
   whole-token appearance in other files.

**Modes:** `plan` returns the flow only (no scan); `analyze` scans and returns
findings + packets; `prove` is accepted and behaves like `analyze` plus a caveat
that LSP/AST proof expansion is not yet run (packets stay candidate-grade — follow
each packet's `next.semantic` to confirm).

**Result row** — one `recordType:"research"` record whose `data` carries both the
raw findings and a decision-grade evidence graph:

```ts
{
  kind: "researchFlow"
  goal: string; intent: ResearchIntent; facets: string[]; mode: "plan" | "analyze" | "prove"; root: string
  flow: Array<{ id; purpose; tools; produces; evidence: "heuristic" | "proof" }>
  summary: { manifests; sourceFiles; entrypoints; reachableFiles; unusedFiles;
    unlistedDependencies; unusedDependencies; duplicateDependencies;
    exportedSymbols; candidateUnusedExports; transitiveDeadExports }
  manifests; files; dependencies; symbols      // raw findings (as before)
  caveats: string[]

  // Evidence graph (analyze/prove modes):
  graphSummary: { subjects; facts; edges; byVerdict; packetsTruncated }
  packets: ResearchEvidencePacket[]            // actionable (dead/unused) first
}

interface ResearchEvidencePacket {
  subject: EvidenceSubject                     // id, kind, name, uri, range (LSP-shaped)
  verdict: "reachable" | "candidate-dead" | "transitive-dead"
         | "candidate-unused-file" | "candidate-unused-dependency" | "unknown"
  proofStatus: "candidate" | "needs-framework-graph" | "conflicting-evidence"
             | "confirmed-by-lsp" | "confirmed-by-ast-and-lsp"
  why: EvidenceFact[]                          // claims with source + confidence
  retainedBy: EvidenceEdge[]                   // what references it (and why it's alive)
  missingProof: Array<{ kind; severity; location? }>   // what's needed to be sure
  risk: { deleteRisk: "low"|"medium"|"high"|"unknown"; reason: string }
  next: Record<string, OqlContinuation>        // next.fetch / next.semantic / next.search
}
```

**Proof rules:** research output is `evidence.kind:"candidate"` and every packet
is `proofStatus:"candidate"` (or `needs-framework-graph`/`conflicting-evidence`).
Reference scoring is token-appearance, not LSP — dynamic imports, framework
entrypoints, tsconfig path aliases, generated files, and test-only retention can
all skew it, so every symbol packet carries `missingProof:["lsp-unavailable"]`
and a `next.semantic` continuation (`lspGetSemantics references`) that **upgrades
the candidate to proof**. Confirm before any deletion; the `caveats` and
`missingProof` state the limits explicitly.

> **Still future:** running the LSP/AST proof expansion automatically
> (`mode:"prove"` confirming refs to set `confirmed-by-lsp`) and a `flows` facet
> (dataflow edges). The candidate graph and the executable path to prove it ship
> today; the automatic proof step does not.

---

## 20. Examples

Local literal search:

```jsonc
{ "schema":"oql","target":"code","from":{"kind":"local","path":"./src"},
  "where":{"kind":"text","value":"runCLI"},
  "select":["path","line","snippet","next.fetch"],"view":"paginated","limit":25 }
```

Local PCRE2 regex:

```jsonc
{ "schema":"oql","target":"code","from":{"kind":"local","path":"./src"},
  "where":{"kind":"regex","value":"function\\s+(?=handle)","dialect":"pcre2"},"view":"detailed" }
```

Local structural with metavar captures:

```jsonc
{ "schema":"oql","target":"code","from":{"kind":"local","path":"./src"},
  "where":{"kind":"structural","lang":"ts","pattern":"class $NAME { $$$BODY }"},
  "select":["path","line","metavars","next.fetch"],"view":"detailed" }
```

Local structural relational rule (await not inside try):

```jsonc
{ "schema":"oql","target":"code","from":{"kind":"local","path":"./src"},
  "where":{"kind":"structural","lang":"ts","rule":{
    "pattern":"await $X",
    "not":{"inside":{"kind":"try_statement","stopBy":"end"}}}} }
```

Boolean over files (contains A and B, not C):

```jsonc
{ "schema":"oql","target":"files","from":{"kind":"local","path":"./src"},
  "where":{"kind":"all","of":[
    {"kind":"text","value":"useEffect"},
    {"kind":"text","value":"useState"},
    {"kind":"not","predicate":{"kind":"text","value":"deprecated"}}]} }
```

GitHub provider search:

```jsonc
{ "schema":"oql","target":"code","from":{"kind":"github","repo":"facebook/react","ref":"main"},
  "scope":{"path":"packages/react","language":["js"]},
  "where":{"kind":"text","value":"useEffect"},
  "materialize":{"mode":"never"},"select":["repo","path","snippet","next.fetch"] }
```

GitHub structural via materialization:

```jsonc
{ "schema":"oql","target":"code","from":{"kind":"github","repo":"facebook/react","ref":"main"},
  "scope":{"path":"packages/react","language":["js"]},
  "where":{"kind":"structural","lang":"js","pattern":"useEffect($$$ARGS)"},
  "materialize":{"mode":"auto","strategy":"subtree"},
  "controls":{"budget":{"maxFiles":500,"maxMaterializedBytes":50000000}},"explain":true }
```

Exact content read:

```jsonc
{ "schema":"oql","target":"content","from":{"kind":"local","path":"./src/index.ts"},
  "fetch":{"content":{"range":{"startLine":40,"endLine":90},"contentView":"exact"}},
  "select":["path","content","next.search"] }
```

GitHub tree:

```jsonc
{ "schema":"oql","target":"structure","from":{"kind":"github","repo":"facebook/react","ref":"main"},
  "scope":{"path":"packages/react"},
  "fetch":{"tree":{"maxDepth":2,"includeSizes":true}},"view":"discovery" }
```

Files by name:

```jsonc
{ "schema":"oql","target":"files","from":{"kind":"local","path":"./packages"},
  "scope":{"language":["ts"],"excludeDir":["node_modules","dist"]},
  "where":{"kind":"field","field":"basename","op":"regex","value":"^(index|main)\\.(ts|tsx)$"},
  "select":["path","size","modified","next.fetch"],"view":"discovery" }
```

LSP references:

```jsonc
{ "schema":"oql","target":"semantics","from":{"kind":"local","path":"./src/index.ts"},
  "params":{"type":"references","symbolName":"runCLI","lineHint":42,"includeDeclaration":false} }
```

Package discovery:

```jsonc
{ "schema":"oql","target":"packages","params":{"packageName":"zod","mode":"full"} }
```

PR detail:

```jsonc
{ "schema":"oql","target":"pullRequests","from":{"kind":"github","repo":"facebook/react"},
  "params":{"prNumber":28000,"reviewMode":"full","filePage":1} }
```

Direct two-ref file diff:

```jsonc
{ "schema":"oql","target":"diff","from":{"kind":"github","repo":"facebook/react"},
  "params":{"baseRef":"v18.2.0","headRef":"v18.3.0","path":"packages/react/index.js"} }
```

Materialize a bounded subtree:

```jsonc
{ "schema":"oql","target":"materialize","from":{"kind":"github","repo":"facebook/react"},
  "scope":{"path":"packages/react"},"materialize":{"mode":"required","strategy":"subtree"} }
```

Smart research:

```jsonc
{ "schema":"oql","target":"research","from":{"kind":"local","path":"."},
  "params":{"goal":"what looks dead, why, and what keeps it alive?","mode":"analyze",
            "facets":["symbols","files","dependencies"]},
  "select":["data"],"view":"paginated","limit":50 }
```

---

## 21. Acceptance gates

1. Every example here parses as `oql`.
2. Every sugar example normalizes to the documented canonical shape.
3. Unknown fields fail; unknown `select` tokens warn (`unknownField`).
4. Reserved targets fail with `unsupportedTarget`.
5. Local text → `localSearchCode fixedString:true`.
6. Local regex → Rust regex or PCRE2 as requested.
7. Local structural → `localSearchCode mode:"structural"`.
8. GitHub structural + `materialize.mode:"never"` → `requiresMaterialization` /
   `materializationNotAllowed`.
9. GitHub structural + `materialize.mode:"auto"` → bounded clone, then local AST.
10. Boolean over `code`/`files` executes via set algebra (not `unsupportedBoolean`);
    `oneOf`/`xor` over budget → `budgetExhausted`.
11. `not`/`xor` over a GitHub provider cannot return `proof` without a complete
    universe or materialized proof.
12. Every partial result returns an executable continuation.
13. `--explain` shows normalized query, defaults, per-node routing, budgets, and
    diagnostics; plan truncation never changes execution.
14. Path validation and secret sanitization remain in every execution path.
15. Batch input preserves per-query diagnostics/provenance/evidence/continuations;
    `combine:"merge"` rejects incompatible rows with a repair.
16. `select` projects fields/continuations; `limit` caps the result-row domain;
    neither changes the result domain or triggers a hidden fetch.

---

## 22. Implementation status & known gaps

OQL exposes every active research target listed in [§3](#3-targets) over the same
envelope, and the planner's `executable` verdict matches what the adapters
actually do (a query that `--explain` calls executable will not collapse to
`requiresMaterialization`/`unsupportedBoolean` at run time). Honest limitations
to plan around:

- **Budgets** `maxCandidates`, `maxBytes`, `maxMaterializedBytes`, and
  `timeoutMs` are accepted but not yet hard-enforced (only `maxFiles`,
  `maxPlanNodes`, and `maxBooleanExpansion` are). They need backend
  cancellation/byte plumbing that does not exist yet.
- **GitHub `files`** lists files *containing a term* approximately (path-level,
  via code search); enumerating by attribute (field), structural, PCRE2,
  negation, or boolean over a GitHub source routes to bounded materialization.
- **`target:"research"`** returns a candidate evidence graph (packets with
  `why`/`retainedBy`/`missingProof`/`risk`/`next`), but reference scoring is
  token-appearance, not LSP — packets are `candidate`-grade. Automatic LSP/AST
  proof expansion (`mode:"prove"` setting `confirmed-by-lsp`) and a `flows` facet
  are still future; follow a packet's `next.semantic` to prove it manually.
- **Human rendering** of `kind:"record"` rows is terser than `--json`; prefer
  `--json` for operation targets.
- **`fixes` / `dataflow`** are reserved (`unsupportedTarget`).

---

## 23. Reserved & future

`target:"fixes"` (dry-run structural codemods — proposed ranges, replacement
text, conflicts, metavariable provenance; never mutating) and
`target:"dataflow"` (candidate `mayFlowTo` with `flowKind:"value"|"taint"`,
sources/sinks/sanitizers/propagators, then engine-backed trace proof) remain
reserved until proof/dry-run engines exist. Candidate flow must return
`evidence.kind:"candidate"` and a diagnostic that prevents vulnerability claims;
engine-backed proof requires traces, source availability, truncation state,
dependency bounds, and provenance. Do not claim Semgrep or ast-grep
compatibility unless a compatibility layer is actually implemented.
