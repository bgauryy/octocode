# Octocode Query Language

OQL is the JSON language behind `octocode search`.

It gives humans and agents one consistent way to ask bounded research questions
over code, files, content, LSP symbols, GitHub repositories, packages, history,
artifacts, diffs, materialized checkouts, smart local research packets, and
relationship graphs.

OQL is not a new parser, a replacement for LSP, or a separate search engine. It
is a typed routing layer over Octocode's existing primitives:

- ripgrep-backed local code and file search;
- structural AST search;
- native graph facts from OXC for JS/TS and tree-sitter for supported source
  languages;
- exact, compact, and symbol-only content reads;
- LSP definitions, references, document symbols, call hierarchy, hover,
  workspace-wide symbol search, type hierarchy (supertypes/subtypes), pull
  diagnostics, and related semantic operations;
- GitHub, npm, history, binary, diff, clone, and cache-backed runners.

The schema name is always `"oql"`. There are no versioned OQL dialects.

Use OQL when the question is broader than one command: multi-step reachability
and dead-code research, cross-target proof, or "find, then prove" workflows. The
research questions ("what's dead, why, what keeps it alive, what proof is
missing") have a dedicated algorithm; see Research And Graph below.

The important rule: OQL can return candidates, proof, and executable next steps.
Do not turn a candidate into a deletion decision until the evidence says the
answer is ready.

## Read This First

Agents should read this document top to bottom the first time. On later uses,
use this path:

1. Choose a `target`.
2. Set `from` and optional `scope`.
3. Use `where` only for code/file predicates.
4. Use `params` for target-specific operations.
5. Use `fetch` for content and tree reads.
6. Keep output small with `view`, `select`, and `controls`.
7. Read `diagnostics`, `provenance`, `evidence`, and `next`.
8. Follow `next.*` continuations instead of inventing follow-up queries.

The live CLI schema is the executable contract:

```bash
octocode search --scheme
octocode search --query '<json>' --json --compact
octocode search --explain --query '<json>' --json --compact
```

Inside this monorepo, the local built CLI is:

```bash
node packages/octocode/out/octocode.js search --scheme
```

MCP exposes the same schema through the thin `oqlSearch` tool. The CLI and MCP
tool must import the shared OQL schema; do not duplicate the shape in an
interface package.

## Minimal Query

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

`schema` may be omitted by callers; normalization inserts `"oql"`.

## Field Roles

| Field | Meaning | Use it for |
|---|---|---|
| `target` | Result family | Choose what kind of answer you want. |
| `from` | Corpus or provider | Local path, GitHub scope, materialized path, or npm. |
| `scope` | Bounds inside the corpus | Path, language, include/exclude globs, depth, hidden/no-ignore. |
| `where` | Code/file predicate | Text, regex, structural AST, file field, boolean combinations. |
| `params` | Target-specific options | LSP type, package query, PR number, artifact mode, research goal, etc. |
| `fetch` | Read instructions | Exact content, compact content, symbol outline, tree settings. |
| `materialize` | Remote-to-local policy | Allow or require bounded GitHub materialization for local proof. |
| `select` | Projection | Return only the fields an agent needs. |
| `view` | Density | Path-only discovery, normal paginated rows, or detailed rows. |
| `controls` | Cost and output controls | Match windows, max matches, budgets, sort, ranking. |
| `limit`, `page`, `itemsPerPage` | Result bounds | Page and cap results. |
| `explain` | Routing visibility | Include normalized query, defaults, plan, backend calls, diagnostics. |

Use the right lane:

| Need | Use | Avoid |
|---|---|---|
| Find code text, regex, or AST shapes | `target:"code"` plus `where` | `fetch` |
| List files by path, extension, or containing/not-containing text | `target:"files"` plus `where` | `fetch` |
| Read a file, range, match window, or symbol outline | `target:"content"` plus `fetch.content` | `where` |
| Browse a tree | `target:"structure"` plus `fetch.tree` | `where` |
| Ask about symbol identity, references, callers, hover, document symbols, type hierarchy, workspace symbols, or file diagnostics | `target:"semantics"` plus `params` | raw text search as final proof |
| Ask broad local research questions | `target:"research"` plus `params.goal` | deleting from candidates alone |
| Ask what keeps a subject alive, what it retains, or what proof is missing | `target:"graph"` plus `params.subject` / `params.verdict` | reading whole files first |

## Transformers

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
should write OQL intent; transformers decide how that intent maps to the vendor
or local primitive.

Examples:

| OQL intent | GitHub code search | GitHub repo/PR search | Local search | npm |
|---|---|---|---|---|
| `scope.language:"ts"` | Prefer `extension:"ts"` for file-level code search. | Use `language:"TypeScript"` only when filtering repository language is intended. | Use TypeScript file/type filters. | No direct field; emit a diagnostic or defer to source-repo follow-up. |
| `scope.language:"typescript"` | Use `language:"TypeScript"` or expand to TypeScript extensions when exact file types are needed. | Use `language:"TypeScript"`. | Expand to the TypeScript extension family. | No direct field. |
| `where.kind:"text"` | `keywords` / provider text query. | PR/repo keyword query when the target supports it. | Ripgrep text search. | Package-name or keyword search when target is `packages`. |
| `where.kind:"structural"` | Not native; materialize first for local AST proof. | Not native. | Structural engine query. | Not native. |
| `fetch.content.contentView:"symbols"` | `ghGetFileContent` with symbol minification. | Not a repo/PR-list field. | `localGetFileContent` symbol view. | Not native. |

Transformers have two separate jobs:

1. Query transformation: convert canonical OQL fields into the best provider
   query without dropping meaning silently.
2. Result transformation: convert provider output back into OQL rows, with
   stable `kind`, `recordType`, `path`, pagination, `next`, diagnostics, and
   evidence.

Keep those jobs separate. A query can be transformed correctly while the result
shape changes underneath it; that must become a `responseShapeMismatch` bug, not
an empty research answer.

Transformer diagnostics must make lossy mappings visible:

| Diagnostic | Meaning |
|---|---|
| `vendorNoEquivalent` | The OQL selector has no direct backend field. |
| `lossyTransform` | The backend query is valid but weaker than the OQL intent. |
| `unsupportedVendorPredicate` | The selected backend cannot evaluate the predicate; materialization or a different target is required. |
| `responseShapeMismatch` | The backing tool returned a shape the transformer did not understand. |

The important rule: never confuse provider dialect with OQL meaning. For
example, `--type ts` can mean an exact `.ts` file extension for GitHub code
search, a broader TypeScript language family for local search, or no direct
constraint for npm. The transformer owns that decision and must expose it in
`--explain`, diagnostics, or provenance when the mapping is approximate.

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
| `graph` | `recordType:"graph"` | Relationship nodes, edges, facts, and missing proof from research packets | OQL graph analyzer |
| `materialize` | `recordType:"materialized"` | Clone/cache a bounded GitHub corpus | `ghCloneRepo` and cache lanes |

Reserved targets:

| Target | Status |
|---|---|
| `fixes` | Reserved until OQL can return safe dry-run edits. |
| `dataflow` | Reserved until OQL can return trace-backed flow proof. |

Reserved targets return `unsupportedTarget`. Do not document or use them as
active language features.

## Target Selection

| User asks | Start with |
|---|---|
| "Find this function/string/pattern" | `target:"code"` |
| "List files matching this" | `target:"files"` |
| "Open this file/range" | `target:"content"` |
| "Show this folder/repo tree" | `target:"structure"` |
| "Where is symbol X referenced?" | `target:"semantics"` |
| "Find packages like X" | `target:"packages"` |
| "Find repos like X" | `target:"repositories"` |
| "What PR changed this?" | `target:"pullRequests"` or `target:"commits"` |
| "Inspect this archive/binary" | `target:"artifacts"` |
| "Compare these refs or PR files" | `target:"diff"` |
| "What looks dead?" | `target:"research"` |
| "What keeps this alive?" | `target:"graph"` |
| "Is this keeper itself dead?" | `target:"graph"` filtered by subject or verdict, then `target:"semantics"` |
| "Make this GitHub subtree local for proof" | `target:"materialize"` |

## Sources

```ts
type QuerySource =
  | { kind: "local"; path: string }
  | { kind: "github"; repo?: string; owner?: string; ref?: string }
  | { kind: "materialized"; localPath: string; source?: QuerySource }
  | { kind: "npm" };
```

Source rules:

- `local.path` is a file or directory on disk.
- `github.repo` is usually `"owner/name"`. `ref` is optional.
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
  include?: string[];
  exclude?: string[];
  excludeDir?: string[];
  hidden?: boolean;
  noIgnore?: boolean;
  maxDepth?: number;
}
```

Use `scope` to bound cost and avoid ambiguous answers. Prefer the smallest
directory, subtree, language set, or include glob that can answer the question.

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

`where` is a discriminated predicate tree. It is used by `code` and `files`.

`target:"code"` requires a `where` predicate. OQL does not treat omitted
`where` as "search everything".

`target:"content"` and `target:"structure"` reject `where`; use `fetch` instead.

### Text

```json
{ "kind": "text", "value": "runCLI" }
```

Fields:

| Field | Values |
|---|---|
| `value` | Required string |
| `case` | `smart`, `sensitive`, `insensitive` |
| `wholeWord` | boolean |
| `id` | optional stable predicate id for diagnostics |

### Regex

```json
{
  "kind": "regex",
  "value": "^export (function|const|type|interface) [A-Za-z0-9_]+",
  "multiline": true
}
```

Fields:

| Field | Values |
|---|---|
| `value` | Required regex string |
| `dialect` | `rust`, `pcre2`, `provider` |
| `case` | `smart`, `sensitive`, `insensitive` |
| `wholeWord` | boolean |
| `multiline` | boolean |
| `dotAll` | boolean |
| `id` | optional stable predicate id |

Defaults: local regex uses the Rust dialect unless a query requests another
dialect that the backend can run.

### Structural AST

```json
{
  "kind": "structural",
  "lang": "typescript",
  "pattern": "function $NAME($$$ARGS): $RET { $$$BODY }"
}
```

Or use a rule:

```json
{
  "kind": "structural",
  "lang": "typescript",
  "rule": {
    "pattern": "eval($X)"
  }
}
```

Fields:

| Field | Values |
|---|---|
| `lang` | Required language id, such as `typescript`, `javascript`, `python`, `rust` |
| `pattern` | Code-shaped AST pattern |
| `rule` | Relational AST rule |
| `id` | optional stable predicate id |

Use exactly one of `pattern` or `rule`.

Structural patterns match complete AST nodes. A function pattern usually needs a
body and any required return-type syntax. Add modifiers such as `export` only
when the resulting pattern still parses as the exact node you want. If a
structural query returns no rows with a `partialParse` or `partialResult`
diagnostic, treat that as missing proof, not proof of absence. Fall back to:

- a narrower structural rule;
- `target:"semantics"` with `documentSymbols`;
- a regex export inventory;
- exact content reads around candidate lines.

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
{
  "kind": "field",
  "field": "extension",
  "op": "=",
  "value": "ts"
}
```

Fields:

| Field | Values |
|---|---|
| `field` | `path`, `basename`, `extension`, `size`, `modified`, `entryType` |
| `op` | `=`, `!=`, `in`, `exists`, `glob`, `regex`, `>`, `>=`, `<`, `<=`, `within` |
| `value` | Required except when `op:"exists"` |
| `id` | optional stable predicate id |

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

## Materialization

```ts
interface MaterializePolicy {
  mode: "never" | "auto" | "required";
  strategy?: "file" | "tree" | "subtree" | "repo";
  allowFullRepo?: boolean;
  forceRefresh?: boolean;
}
```

Modes:

| Mode | Meaning |
|---|---|
| `never` | Do not clone/cache remote code for local proof. |
| `auto` | Materialize only when required to answer exactly. |
| `required` | Fail if the bounded corpus cannot be materialized. |

Default for GitHub sources is `never`.

Use materialization when a GitHub provider cannot prove what the user asked:

- structural AST search over remote code;
- PCRE2/local-only regex behavior;
- file-set negation;
- LSP semantics over remote code;
- complete local proof for a bounded subtree.

Always bound materialization with `scope.path`, `scope.include`, or a similarly
small corpus. Avoid full-repo materialization unless the user asked for it and
the repo size is acceptable.

## Fetch

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
    maxDepth?: number;
    includeSizes?: boolean;
  };
}
```

Content views:

| View | Meaning |
|---|---|
| `exact` | Exact source text, best for citations and patches. |
| `compact` | Minified/compact content, default for token efficiency. |
| `symbols` | File outline or symbol skeleton, best for orientation. |

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
the backing tool remains the exhaustive validator.

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

Fields:

| Field | Values |
|---|---|
| `type` | `definition`, `references`, `callers`, `callees`, `callHierarchy`, `hover`, `documentSymbols`, `typeDefinition`, `implementation`, `workspaceSymbol`, `supertypes`, `subtypes`, `diagnostic` |
| `uri` | optional file uri/path override (required for all types except `workspaceSymbol`) |
| `symbolName` | symbol to resolve near `lineHint`; fuzzy search query for `workspaceSymbol` |
| `lineHint` | 1-based line anchor (required except for `documentSymbols`, `workspaceSymbol`, `diagnostic`) |
| `orderHint` | integer disambiguation hint |
| `includeDeclaration` | boolean |
| `depth` | integer 0-20 |
| `groupByFile` | boolean |
| `workspaceRoot` | optional workspace root |
| `format` | `structured` or `compact` |
| `page`, `itemsPerPage` | pagination |

**LSP 3.17 additions:**

| Type | Anchor | Use when |
|---|---|---|
| `workspaceSymbol` | `symbolName` only (no file or position needed) | Find all symbols matching a name project-wide; useful when you don't know the file |
| `supertypes` | `uri` + `symbolName` + `lineHint` | Walk up an inheritance chain (what does this class/interface extend?) |
| `subtypes` | `uri` + `symbolName` + `lineHint` | Walk down an inheritance chain (what implements this interface?) |
| `diagnostic` | `uri` only | Pull errors/warnings for a file from the language server (LSP pull model) |

`diagnostic` requires a language server that supports the pull-diagnostic protocol
(`textDocument/diagnostic`, LSP 3.17). Servers that only push diagnostics
(`textDocument/publishDiagnostics`) return an `unsupportedOperation` payload with
a migration hint.

For deletion/reachability questions, `references` with
`includeDeclaration:false` is the key proof operation.

### `repositories`

Backs onto `ghSearchRepos`.

Fields:

| Field | Values |
|---|---|
| `keywords` | string array |
| `topicsToSearch` | string array |
| `language` | string |
| `owner` | string |
| `stars` | string or number |
| `size` | string |
| `updated` | string |
| `license` | string |
| `visibility` | `public` or `private` |
| `archived` | boolean |
| `sort` | `stars`, `forks`, `help-wanted-issues`, `updated`, `best-match` |
| `concise` | boolean |
| `page` | positive integer |

### `packages`

Backs onto `npmSearch`.

Fields:

| Field | Values |
|---|---|
| `packageName` | exact or near package name |
| `keywords` | string array |
| `mode` | `lean` or `full` |
| `page` | positive integer |

Use either `packageName` or `keywords`.

### `pullRequests`

Backs onto `ghHistoryResearch`.

Fields:

| Field | Values |
|---|---|
| `prNumber` | positive integer |
| `state` | `open`, `closed`, `merged` |
| `author` | string |
| `label` | string or string array |
| `keywordsToSearch` | string array |
| `head`, `base` | branch/ref filters |
| `reviewMode` | backing tool review mode |
| `filePage`, `commentPage`, `commitPage` | positive integers |
| `charOffset`, `charLength` | patch/content paging |
| `minify` | `none` or `standard` |
| `limit`, `page` | pagination |

### `commits`

Backs onto `ghHistoryResearch` with commit mode.

Fields:

| Field | Values |
|---|---|
| `path` | optional path filter |
| `branch` | optional branch |
| `since`, `until` | date strings accepted by the backing tool |
| `includeDiff` | boolean |
| `limit`, `page` | pagination |

### `artifacts`

Backs onto `localBinaryInspect`.

Fields:

| Field | Values |
|---|---|
| `mode` | `inspect`, `list`, `extract`, `decompress`, `strings`, `unpack` |
| `archiveFile` | archive entry/path selector |
| `entryPageNumber`, `entriesPerPage` | archive entry pagination |
| `minLength` | string-scan minimum length, 1-128 |
| `scanOffset` | string scan continuation offset |
| `charOffset`, `charLength` | output slicing |
| `matchString` | string match filter |
| `verbose` | boolean |

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

If neither lane is present, OQL returns an `invalidQuery` diagnostic with a
repair message.

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

Fields:

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

The research row can include:

- `summary`;
- `graphCapabilities`;
- `nativeGraphSummary`;
- `graphSummary`;
- `packetPage`;
- `packets`;
- `caveats`.

In `view:"detailed"`, the research row can also include raw analyzer arrays:

- `manifests`;
- `files`;
- `dependencies`;
- `symbols`.
- `graphFacts`.

Each packet can include:

- `subject`;
- `verdict`;
- `proofStatus`;
- `why`;
- `retainedBy`;
- `missingProof`;
- `risk`;
- `next`.

The research result row itself can also carry `next.graph`. That row-level
continuation is page-aligned and upgrades the current packet page to
`target:"graph"` with bounded LSP proof. Packet-level `next` entries are for
exact reads, semantic checks, or text/dynamic follow-up on the individual
candidate.

Important: research packets use native AST facts where available. The source
universe is capability-driven, not JS/TS-hardcoded: JS/TS uses OXC graph facts,
and other supported source languages use tree-sitter graph inventory. The row's
`graphCapabilities` tells agents which extensions can emit graph facts, which
languages were seen, and whether any source files missed graph extraction.

AST graph facts prove syntax inventory: declarations, imports, exports/public
hints, calls, containment, and source locations. Cross-file references and
callers are still candidate evidence until LSP proof is attached or followed.
Treat packet deletion claims as final only when LSP proof has been attached or
followed and the packet proof status supports the conclusion.

If `pagination.hasMore:true`, follow top-level `next.page` before calling the
packet set complete. Research keeps the packet universe available; use
top-level `page` and `itemsPerPage` to control response size.

### `graph`

The graph target returns an agent-readable relationship graph built from the
same research packet universe as `target:"research"`, enriched by native AST
graph facts where available. Use it when the question is about relationships
rather than the full packet list:

- what retains this symbol or file;
- what this subject retains;
- which candidate-dead nodes are only kept alive by other candidate-dead nodes;
- which proof is missing for the current page of relationships;
- what exact packet continuations should be followed next.

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

Fields:

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
| `proof` | `none` or `lsp`; `lsp` asks the graph adapter to run LSP reference proof for symbol packets on the current page |
| `proofLimit` | max current-page symbols to prove with LSP, capped at 25; default is up to 5 |
| `includePackets` | include paged packets with `next.*` continuations; default `true` |
| `includeFacts` | include `why` facts; default `true` |
| `includeEdges` | include relationship edges; default `true` |
| top-level `page` / `itemsPerPage` | page through the filtered packet domain |

The graph row can include:

- `nodes`: subjects such as files, symbols, functions, classes, dependencies,
  packages, and entrypoints;
- `edges`: relationship facts between subjects, such as `references`,
  `imports`, `calls`, `retains`, and `reachableFrom`;
- `facts`: direct evidence attached to subjects;
- `missingProof`: proof still needed for the current graph page;
- `packets`: optional packet rows with `why`, `retainedBy`, `risk`, and
  `next.*` continuations;
- `nativeGraphSummary`: count of files, declarations, imports, exports, calls,
  and edges extracted by the native AST graph-fact primitive;
- `graphCapabilities`: supported graph-fact extensions, fact families, language
  coverage for the current corpus, and missing graph-fact coverage;
- `summary`, `graphSummary`, and `packetPage`.

Important: graph proof is page-bounded. `params.proof:"lsp"` and
`params.mode:"prove"` can attach `packets[].proof.lsp`, remove
`lsp-unavailable` from proved packets, and change packet `proofStatus`.
Unproved pages and paginated LSP results remain missing proof. Use graph rows
to decide exactly what to inspect next, then follow packet `next.semantic`,
`next.fetch`, or `next.search` continuations before saying code is safe to
delete.

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
`ref`, `cache`, and `complete`, plus continuations for local structure/files.

## Views, Select, And Controls

Views:

| View | Meaning |
|---|---|
| `discovery` | Smallest output. Prefer paths, identities, and continuations. |
| `paginated` | Default balanced result rows. |
| `detailed` | More context, snippets, and richer payloads. |

`select` projects fields from result rows and continuations:

```json
{
  "select": ["path", "line", "snippet", "next.fetch", "next.semantic"]
}
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

Batch fields:

| Field | Values |
|---|---|
| `queries` | 1-5 `OqlQuery` objects |
| `combine` | `independent` or `merge` |
| `limit`, `page`, `itemsPerPage` | optional batch result bounds |
| `explain` | include plans |

## Normalization And Explain

Raw input can use a small amount of sugar, but canonical OQL is strict. The
normalizer:

- inserts `schema:"oql"`;
- infers `target` from safe sugar only when deterministic;
- rewrites accepted sugar into canonical fields;
- rejects unknown fields with `unknownField`;
- rejects ambiguous sugar with `ambiguousSugar`;
- rejects reserved targets with `unsupportedTarget`;
- validates common target `params`;
- returns the strict normalized query in `--explain`.

Use `--explain` when:

- a query mixes boolean predicates;
- a GitHub query may require materialization;
- a structural query may not be exact;
- an answer depends on negation;
- an agent is about to claim absence or safe deletion.

Explain includes:

- `input`;
- `normalized`;
- `defaults`;
- `nodes`;
- `backendCalls`;
- `materialization`;
- `budgets`;
- `diagnostics`;
- `next`.

Plan routes:

| Route | Meaning |
|---|---|
| `PUSHDOWN` | Backend can evaluate exactly. |
| `RESIDUAL` | Backend can narrow candidates, but OQL must finish locally. |
| `ROUTE` | OQL must use another lane, often materialization. |
| `UNSUPPORTED` | OQL cannot execute the requested semantics safely. |

## Result Envelope

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

`proofGrade` is mandatory per row: `candidate`, `text`, `structural`,
`semantic`, `graph`, or `missing`. Projection never removes it.

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

Evidence:

| Evidence | Meaning |
|---|---|
| `proof` | Backends evaluated the request exactly enough for the stated answer. |
| `partial` | More pages, truncation, or residual checks remain. |
| `candidate` | Useful candidate evidence, but not final proof. |
| `unsupported` | OQL could not safely execute the requested semantics. |

`answerReady:true` means the envelope can answer the query as asked.
`complete:true` means the envelope is not missing required pages or proof work.

## Diagnostics

Diagnostics are part of the answer. Agents must read them.

Common codes include:

| Code | Meaning |
|---|---|
| `invalidQuery` | Query shape or params are invalid. |
| `unknownField` | Field is not part of OQL. |
| `unsupportedTarget` | Target is reserved or inactive. |
| `unsupportedPredicate` | Predicate cannot run on that target/source. |
| `negativeUniverseRequired` | Negation needs a complete local/materialized universe. |
| `requiresMaterialization` | Exact proof needs materialization. |
| `materializationNotAllowed` | Query needs materialization but mode forbids it. |
| `providerSemanticsApproximate` | Provider filter is useful but not exact proof. |
| `partialResult` | Result is incomplete or candidate-grade. |
| `contentTruncated` | Content was sliced. |
| `matchTruncated` | Match snippet was sliced. |
| `planTruncated` | Explain plan was capped. |
| `budgetExhausted` | Budget stopped the query. |
| `parserFailed` | Parser failed. |
| `partialParse` | Parser skipped or partially parsed inputs. |
| `lspUnavailable` | LSP server or capability unavailable. |
| `sanitized` | Secrets or sensitive content were redacted. |
| `rateLimited` | Provider rate limit. |
| `zeroMatches` | No matches. Verify scope before treating as absence. |

If a diagnostic has `repair.suggestedQuery`, prefer that over inventing a new
shape.

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

Common continuation names:

| Continuation | Meaning |
|---|---|
| `next.fetch` | Read exact content or an outline. |
| `next.semantic` | Ask LSP about the symbol/file. |
| `next.search` | Run a text/regex follow-up. |
| `next.page` | Continue the primary result domain, including research packet pages. |
| `next.charRange` | Continue a content range. |
| `nextScanOffset` inside artifact data | Continue a strings scan. |

Agents should follow continuations because they carry path, range, source, and
reasoning already validated by OQL.

## Research And Graph

`target:"research"` and `target:"graph"` answer reachability and dead-code
questions: "what looks dead, why, what keeps it alive, is that keeper itself
dead, what proof is missing, what to inspect next". Both are candidate-first:
promote candidates with `next.semantic` / `next.search` / `next.fetch` before
any deletion claim. Text and ripgrep are discovery only, never deletion-grade
proof.

Query shape is the `research` and `graph` entries under Params By Target. The
full algorithm (structure -> discovery -> AST -> LSP -> graph -> packet),
evidence tiers, verdicts, graph-capability fields, language coverage, and the
question-to-field map are the canonical research contract:
https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OQL_RESEARCH_GRAPH_FLOW.md

OQL beats a single knip-style command when the agent must ask "why?" and continue
into exact proof; a dedicated tool is better for a one-shot, framework-aware
entrypoint/dependency audit.

## Examples

### Local Text Search

```json
{
  "schema": "oql",
  "target": "code",
  "from": { "kind": "local", "path": "./packages/octocode/src" },
  "where": { "kind": "text", "value": "runCLI" },
  "select": ["path", "line", "snippet", "next.fetch"],
  "view": "paginated",
  "limit": 20
}
```

### Enumerate Exports With Regex

```json
{
  "schema": "oql",
  "target": "code",
  "from": { "kind": "local", "path": "./src" },
  "where": {
    "kind": "regex",
    "value": "^export (function|const|type|interface) [A-Za-z0-9_]+",
    "multiline": true
  },
  "select": ["path", "line", "snippet", "next.semantic"],
  "view": "detailed",
  "limit": 50
}
```

### Structural Function Search

```json
{
  "schema": "oql",
  "target": "code",
  "from": { "kind": "local", "path": "./src" },
  "where": {
    "kind": "structural",
    "lang": "typescript",
    "pattern": "function $NAME($$$ARGS): $RET { $$$BODY }"
  },
  "select": ["path", "line", "snippet", "metavars", "metavarRanges"],
  "view": "detailed"
}
```

### LSP Document Symbols

```json
{
  "schema": "oql",
  "target": "semantics",
  "from": { "kind": "local", "path": "./src/index.ts" },
  "params": {
    "type": "documentSymbols",
    "format": "structured"
  },
  "view": "detailed"
}
```

### LSP References

```json
{
  "schema": "oql",
  "target": "semantics",
  "from": { "kind": "local", "path": "./src/index.ts" },
  "params": {
    "type": "references",
    "symbolName": "runCLI",
    "lineHint": 42,
    "includeDeclaration": false,
    "groupByFile": true,
    "format": "structured"
  },
  "view": "detailed"
}
```

### Files By Extension

```json
{
  "schema": "oql",
  "target": "files",
  "from": { "kind": "local", "path": "./src" },
  "where": {
    "kind": "field",
    "field": "extension",
    "op": "=",
    "value": "ts"
  },
  "select": ["path", "entryType", "modified", "next.fetch"],
  "view": "discovery"
}
```

### Files That Do Not Contain Text

```json
{
  "schema": "oql",
  "target": "files",
  "from": { "kind": "local", "path": "./src" },
  "where": {
    "kind": "all",
    "of": [
      { "kind": "field", "field": "extension", "op": "=", "value": "ts" },
      { "kind": "not", "predicate": { "kind": "text", "value": "MCP_REGISTRY" } }
    ]
  },
  "view": "discovery"
}
```

### GitHub Code Search

```json
{
  "schema": "oql",
  "target": "code",
  "from": { "kind": "github", "repo": "owner/name", "ref": "main" },
  "scope": { "path": "src" },
  "where": { "kind": "text", "value": "createServer" },
  "select": ["path", "snippet", "next.fetch"],
  "view": "paginated"
}
```

### GitHub Structural Proof With Materialization

```json
{
  "schema": "oql",
  "target": "code",
  "from": { "kind": "github", "repo": "owner/name", "ref": "main" },
  "scope": { "path": "src" },
  "where": {
    "kind": "structural",
    "lang": "typescript",
    "pattern": "eval($X)"
  },
  "materialize": { "mode": "auto", "strategy": "subtree" },
  "view": "detailed"
}
```

### Package Search

```json
{
  "schema": "oql",
  "target": "packages",
  "from": { "kind": "npm" },
  "params": {
    "keywords": ["typescript", "lsp"],
    "mode": "lean"
  },
  "view": "discovery"
}
```

### PR Patch Diff

```json
{
  "schema": "oql",
  "target": "diff",
  "from": { "kind": "github", "repo": "owner/name" },
  "params": {
    "prNumber": 123,
    "files": ["src/index.ts"]
  },
  "view": "detailed"
}
```

### Artifact Strings

```json
{
  "schema": "oql",
  "target": "artifacts",
  "from": { "kind": "local", "path": "./dist/app.bin" },
  "params": {
    "mode": "strings",
    "minLength": 6
  },
  "view": "paginated"
}
```

## Agent Rules

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

## Safe Deletion Rules

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

A good OQL answer can say:

- "candidate dead";
- "proof missing: LSP references";
- "retained by file X line Y";
- "keeper is also unreferenced";
- "safe to inspect next";
- "not safe to delete yet".

It should not say "delete this" when `evidence.kind:"candidate"` or
`answerReady:false`.

## Current Limits To Tell Agents

- `target:"research"` is the right entrypoint for knip-like questions, but the
  current packet is candidate-first. Use `next.semantic`, `next.search`, and
  `next.fetch` to upgrade evidence.
- `target:"graph"` is the right entrypoint for retained-by chains and
  relationship questions. Use `params.proof:"lsp"` or `mode:"prove"` to run
  bounded LSP proof for the current page, then keep paging/following
  continuations until missing proof is closed.
- Native graph inventory is capability-driven across supported OXC/tree-sitter
  source languages. Read `graphCapabilities` and `nativeGraphSummary` before
  claiming absence; missing capability is not proof of absence.
- `mode:"prove"` on `target:"graph"` is page-bounded. Unproved pages,
  unavailable LSP servers, paginated LSP results, dynamic imports, and framework
  entrypoints remain `missingProof`.
- Tree-sitter graph facts are syntax inventory. Public/export hints and call
  edges are language-aware candidates, not semantic proof. LSP references,
  definitions, and call hierarchy are the proof layer.
- Rich `research` and `graph` rows can still return packet-shaped data. Keep
  `facets`, `maxFiles`, `page`, `itemsPerPage`, and `view` tight.
- Structural AST search is exact only when the pattern/rule is accepted by the
  parser and diagnostics are clean.
- File and dependency deletion still need project-specific entrypoint,
  framework, script, dynamic import, and generated-file awareness.
- LSP proof is only as complete as the workspace, language server, and symbol
  anchor provided.

## One-Screen Agent Checklist

Before answering:

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
