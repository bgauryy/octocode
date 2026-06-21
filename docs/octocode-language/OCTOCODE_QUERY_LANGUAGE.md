# Octocode Query Language

<oql_system_prompt id="octocode-oql-v1" version="oql/v1" audience="agents">

<section id="purpose">

## Purpose

OQL is the typed research query object behind `octocode search`.

Implementation status: V1 + V2 targets are implemented. The OQL engine
(schema, normalizer, capability registry, planner/explain, execution adapters,
materialization lane, result envelope) lives in
`@octocodeai/octocode-tools-core/oql`, surfaced as `octocode search`. Active
targets: `code`, `content`, `structure`, `files` (V1) plus `semantics`,
`repositories`, `packages`, `pullRequests`, `commits`, `artifacts`, `diff`
(V2 — backed by lspGetSemantics, ghSearchRepos, npmSearch, ghHistoryResearch,
localBinaryInspect; remote semantics route through materialization). V2 targets
take a typed `params` bag for backend-specific options. Only `fixes`/`dataflow`
(V3) remain reserved (`unsupportedTarget`). Existing quick commands and raw
`tools NAME` calls remain available; quick-command lowering into OQL is future.

This document is both:

- an implementation contract for OQL V1;
- a system-prompt style guide for agents that need to construct, normalize,
  explain, execute, and continue research queries.

Future work lives in:
https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_QUERY_LANGUAGE_PLAN.md

</section>

<section id="runner-protocol">

## Runner Protocol

<runner_contract>

`octocode search` is implemented for OQL V1 (local + GitHub code/content/
structure/files, with bounded remote-as-local materialization). The OQL engine
lives in `@octocodeai/octocode-tools-core/oql` (`runOqlSearch`); the CLI command
is a thin wrapper. This is the CLI/API contract it provides.

| Command | Meaning |
|---|---|
| `octocode search --scheme` | print the OQL input and result schemas |
| `octocode search --query JSON` | run one OQL input object |
| `octocode search --file ./query.json` | run OQL input from a file |
| `cat query.json | octocode search --stdin` | run OQL input from stdin |
| `octocode search --explain --query JSON` | normalize and plan; execution may be skipped only with `--dry-run` |
| `octocode search --json` | return the typed result envelope |
| `octocode search --compact` | render the lowest-token human/agent view |

Exit codes should follow the existing CLI convention: `0` ok, `2` bad input,
`3` not found, `4` auth, `5` tool error, `7` rate limited.

</runner_contract>

</section>

<section id="agent-search-description">

## Agent Search Description

<description_string>

`octocode search` answers bounded research questions over local paths and
GitHub scopes. Use it to search for code matches (`target:"code"`), file lists
(`target:"files"`), directory trees (`target:"structure"`), or exact/minified
file content (`target:"content"`). Set `from` to a local, GitHub, or
materialized source; narrow with `scope`; express the question with
`where.kind` (`text`, `regex`, `structural`, `field`, `all`, `any`, `not`);
control density with `view`, `select`, and `controls`; and use
`materialize.mode:"auto"` or `"required"` only when a bounded GitHub scope needs
local proof. When routing or completeness is uncertain, run `--explain` and
follow returned `next.*` continuations instead of inventing pages, paths, or
offsets.

</description_string>

</section>

<section id="attention">

## Attention

<attention priority="highest">

Use **canonical OQL** for implementation. Accept sugar only at the CLI/API edge,
then normalize it before planning. `octocode search --explain` must show the
canonical query that actually runs.

</attention>

<must>

- Use one universal runner: `octocode search`.
- Use one canonical shape: `target`, `from`, `scope`, `where.kind`,
  `materialize`, `fetch`, `select`, `view`, `controls`, result bounds, and
  `explain`.
- Make every `where` leaf a discriminated predicate with `kind`.
- Keep path validation and secret sanitization in every execution path.
- Return typed diagnostics and executable continuations.

</must>

<must_not>

- Do not invent backend behavior.
- Do not drop predicates.
- Do not silently materialize.
- Do not silently run LSP.
- Do not silently run a weaker query.
- Do not treat provider zero results as proof unless every predicate was
  evaluated by the provider.
- Do not mutate files from OQL V1.

</must_not>

</section>

<section id="agent-decision-loop">

## Agent Decision Loop

<agent_loop>

1. Identify the research target: `code`, `content`, `structure`, or `files`.
   For V2-only research surfaces such as semantic/LSP navigation, repositories,
   packages, PR/history, binary, or diff, use the reserved target map in this
   document and expect V1 to return a typed `unsupportedTarget` diagnostic.
2. Identify the corpus in `from`.
3. Bound the corpus in `scope`.
4. Express the logical question in `where.kind`.
5. Decide whether external-to-local execution is allowed through
   `materialize.mode`.
6. Choose output density with `view`, projection with `select`, and cost
   controls with `controls`.
7. Run or request `--explain`.
8. Follow executable `next.*` continuations instead of inventing paths, pages,
   offsets, or follow-up queries.
9. Treat diagnostics as evidence about capability and completeness.

</agent_loop>

</section>

<section id="v1-surface">

## V1 Surface

<active_surface>

| Family | V1 status | Current backing |
|---|---|---|
| local code search | active | `localSearchCode` |
| local structural search | active | `localSearchCode mode:"structural"` |
| local content read | active | `localGetFileContent` |
| local structure | active | `localViewStructure` |
| local file discovery | active | `localFindFiles` |
| GitHub code search | active | `ghSearchCode` |
| GitHub content read | active | `ghGetFileContent` |
| GitHub structure | active | `ghViewRepoStructure` |
| bounded GitHub remote-as-local proof | active | `ghCloneRepo` plus local tools |
| LSP, repos, packages, PR/history, binary, diff | V2 | plan only |
| fixes and dataflow | V3 | plan only |

</active_surface>

</section>

<section id="research-capability-coverage">

## Research Capability Coverage

<capability_matrix>

| Capability | Current Octocode surface | OQL status | OQL representation |
|---|---|---|---|
| local text search | `localSearchCode`, `grep` | V1 active | `target:"code"` + `where.kind:"text"` |
| local regex search | `localSearchCode`, `grep --perl-regex` | V1 active | `target:"code"` + `where.kind:"regex"` + `dialect` |
| local structural AST search | `localSearchCode mode:"structural"`, `grep --pattern/--rule` | V1 active | `target:"code"` + `where.kind:"structural"` |
| local file discovery | `localFindFiles`, `find` | V1 active | `target:"files"` + `scope` + field predicates |
| local tree browsing | `localViewStructure`, `ls` | V1 active | `target:"structure"` + `fetch.tree` |
| local content and minification | `localGetFileContent`, `cat --mode` | V1 active | `target:"content"` + `fetch.content.contentView` |
| signature/symbol outline from text | `localGetFileContent minify:"symbols"`, `ghGetFileContent minify:"symbols"`, `cat --mode symbols` | V1 active | `target:"content"` + `fetch.content.contentView:"symbols"` |
| GitHub code search | `ghSearchCode`, `grep owner/repo` | V1 active | `from.kind:"github"` + `target:"code"` |
| GitHub content read | `ghGetFileContent`, `cat owner/repo/path` | V1 active | `from.kind:"github"` + `target:"content"` |
| GitHub tree browsing | `ghViewRepoStructure`, `ls owner/repo` | V1 active | `from.kind:"github"` + `target:"structure"` |
| bounded remote-as-local proof | `ghCloneRepo`, `grep --repo`, cache fetch handoff | V1 active for GitHub code/content/tree | `materialize.mode:"auto"` or `"required"` then local backend |
| LSP definitions/references/callers/callees/types | `lspGetSemantics`, `lsp` | V2 reserved | target family `semantics` with an explicit semantic operation |
| LSP document symbols | `lspGetSemantics type:"documentSymbols"` | V2 reserved | target family `semantics`, operation `documentSymbols` |
| repository discovery | `ghSearchRepos`, `repo` | V2 reserved | target family `repositories` |
| npm package discovery/source handoff | `npmSearch`, `pkg` | V2 reserved | target family `packages` |
| PR and commit history | `ghHistoryResearch`, `pr`, `history` | V2 reserved | target families `pullRequests` and `commits` |
| binary/archive/string inspection | `localBinaryInspect`, `binary`, `unzip` | V2 reserved | target family `artifacts` |
| diff research | `diff`, PR patch selectors | V2 reserved | target family `diff` |
| dry-run fixes and dataflow | no V1 proof engine | V3 reserved | target families `fixes` and `dataflow` only after proof support |

</capability_matrix>

<coverage_rules>

- The V1 contract covers code/content/structure/files across local and GitHub,
  including AST, PCRE2, exact content proof, pagination, and bounded
  remote-as-local execution.
- The V1 contract does not cover all existing Octocode commands. It must say this
  explicitly through `unsupportedTarget` rather than pretending those commands
  already have OQL parity.
- Reserved V2 target families must reuse the same top-level grammar:
  `target`, `from`, `scope`, `where`, `materialize`, `fetch`, `select`, `view`,
  `controls`, diagnostics, provenance, and continuations.
- Do not create a new mini-language for LSP, npm, history, binary, or diff.
  Add typed target-specific fields only when the common grammar cannot express
  the operation.

</coverage_rules>

</section>

<section id="borrow-and-defer">

## Borrow And Defer

<borrow_defer_matrix>

| Prior art | Borrow now | Defer |
|---|---|---|
| ripgrep | literal/regex/PCRE2, globs, ignores, hidden files, context, counts, only-matching, match pages, multiline, caps | replacements, encodings, compressed-file search as search, unrestricted binary search |
| ast-grep | code-shaped `pattern`, JSON `rule`, `kind`, `inside`, `has`, `not`, `all`, `any`, `stopBy:"end"` | `precedes`, `follows`, `field`, `regex`, `nthChild`, `range`, `matches`, utility rules, strictness modes |
| Semgrep | explicit rule evaluation language and future metavariable constraints | Semgrep-compatible syntax, `focus`, `fix`, `transform`, taint in V1 |
| CodeQL | separation between syntax proof and flow proof | dataflow/taint until an engine can return traces and proof provenance |
| GitHub/Sourcegraph search | typed provider filters and cheap path/code pushdown | string DSL compatibility, provider `symbol:` as reference proof, `select` changing result domains |
| LSP | position-based semantic operations, server capability diagnostics | treating symbol-name search as proof without a real file/line anchor |

</borrow_defer_matrix>

<borrow_defer_rules>

- OQL is a typed research object, not a string DSL.
- `select` is projection only. It must not deduplicate, change result domains, or
  trigger hidden fetches.
- AST proves syntax shape. LSP proves semantic relations. Dataflow proves flow
  only after a flow engine returns traces.
- Provider-specific filters are pushdown opportunities, not proof unless the
  provider can evaluate the exact predicate.

</borrow_defer_rules>

</section>

<section id="canonical-query">

## Canonical Query

<canonical_schema>

```ts
type OqlActiveTargetV1 = "code" | "content" | "structure" | "files"

type OqlReservedTarget =
  | "semantics"
  | "repositories"
  | "packages"
  | "pullRequests"
  | "commits"
  | "artifacts"
  | "diff"
  | "fixes"
  | "dataflow"

interface OqlQueryV1 {
  schema: "oql/v1"
  id?: string
  target: OqlActiveTargetV1
  from: QuerySource
  scope?: QueryScope
  where?: Predicate
  materialize?: MaterializePolicy
  fetch?: FetchInstructions
  select?: SelectField[]
  view?: "discovery" | "paginated" | "detailed"
  controls?: QueryControls
  limit?: number
  page?: number
  itemsPerPage?: number
  explain?: boolean
}

interface OqlBatchV1 {
  schema: "oql/v1"
  id?: string
  queries: OqlQueryV1[]
  combine?: "independent" | "merge"
  limit?: number
  page?: number
  itemsPerPage?: number
  explain?: boolean
}

interface OqlInputQueryV1 extends Omit<OqlQueryV1, "schema" | "target"> {
  schema?: "oql/v1"
  target: OqlActiveTargetV1 | OqlReservedTarget
}

interface OqlInputBatchV1 extends Omit<OqlBatchV1, "schema" | "queries"> {
  schema?: "oql/v1"
  queries: OqlInputQueryV1[]
}

type OqlSearchInputV1 = OqlInputQueryV1 | OqlInputBatchV1

type OqlCanonicalInputV1 = OqlQueryV1 | OqlBatchV1

type SelectField = string
```

</canonical_schema>

<agent_safe_subset>

Agents should author this subset first. Everything else is power-user or
lowering output.

| Need | Canonical form |
|---|---|
| find code | `target:"code"`, `from`, `where`, optional `scope.language/path` |
| list files | `target:"files"`, `from`, optional field predicates |
| read content | `target:"content"`, `from`, `fetch.content` |
| browse tree | `target:"structure"`, `from`, `fetch.tree` |
| remote local proof | add `materialize:{mode:"auto",strategy:"subtree"}` |
| cheap paths only | `view:"discovery"` plus `select:["path","next.fetch"]` |
| exact proof | follow `next.fetch` with `fetch.content.contentView:"exact"` |

Rules:

- Prefer `view` for density, `select` for projection, and `controls` for cost.
- Do not hand-author backend-like shortcuts such as `filesOnly`; the normalizer
  may accept them as legacy sugar but `--explain` must emit canonical fields.
- When unsure, request `explain:true` and let repair diagnostics point to the
  narrower canonical form.

</agent_safe_subset>

<field_contract>

| Field | Meaning |
|---|---|
| `schema` | language version; optional on raw input, required after normalization |
| `id` | optional stable query id; compiler assigns one when omitted |
| `target` | result family |
| `from` | corpus identity |
| `scope` | allowed corpus slice |
| `where` | logical match predicate |
| `materialize` | permission and strategy for external-to-local execution |
| `fetch` | bytes, tree, or content acquisition options |
| `select` | returned fields |
| `view` | output density, not match semantics |
| `controls` | budget, paging domains, ranking, snippets, debug |
| `limit`, `page`, `itemsPerPage` | result-level bounds |
| `explain` | return plan and normalized query |

</field_contract>

<versioning_rules>

- `schema:"oql/v1"` rejects unknown canonical fields.
- Raw input may omit `schema`; normalization inserts `schema:"oql/v1"`.
- V2/V3 fields are not allowed to appear silently inside V1 objects.
- Raw input may name a reserved target. Normalization preserves enough
  information to return `unsupportedTarget`; canonical V1 execution only accepts
  active targets.
- Experimental fields must live under a namespaced `extensions` object in future
  schemas, not at arbitrary top-level positions.
- A future `schema:"oql/v2"` may add fields, targets, and predicates, but V1
  parsers must continue to reject them unless explicitly run in a compatibility
  mode.

</versioning_rules>

<bulk_contract>

`octocode search` accepts one canonical query or a bounded batch. Batching is
for independent research slices, not hidden cross-query logic.

| Field | Meaning |
|---|---|
| `queries` | ordered list of OQL queries; V1 should use the existing tool-call cap of 1-5 queries |
| `combine:"independent"` | default; return one result envelope per query |
| `combine:"merge"` | merge compatible rows only after each query keeps its own provenance and diagnostics |
| batch `limit`, `page`, `itemsPerPage` | outer bounds for merged rendering; inner query bounds still apply |
| batch `explain` | explain every child query and the batch merge plan |

Rules:

- Query order is stable.
- A failed query does not erase successful sibling results.
- Each query gets its own diagnostics, provenance, evidence, pagination, and
  continuations.
- `combine:"merge"` is valid only when all child queries return compatible
  result rows. Otherwise return `invalidQuery` with a repair hint to use
  `combine:"independent"`.

Compatible rows means:

- same `target`;
- same evaluation unit;
- same normalized `select` set or no explicit `select`;
- same primary pagination domain;
- row identity can be computed from the same stable keys such as
  `source/repo/localPath/path/line/endLine/column`.

Merged output must preserve child query IDs and provenance. It may deduplicate
only by stable row identity.

</bulk_contract>

<target_contract>

| Target | Required | Optional | Does not use |
|---|---|---|---|
| `code` | `from`, `where` | `scope`, `materialize`, `select`, `view`, `controls`, result bounds | `fetch.content`, `fetch.tree` unless emitted as continuation |
| `content` | `from` | `fetch.content`, `select`, `view`, result bounds | `where` except future normalized match sugar |
| `structure` | `from` | `scope.path`, `fetch.tree`, `select`, `view`, result bounds | `where` except field/path filters after implementation supports them |
| `files` | `from` | `scope`, field/text/regex/structural `where`, `select`, `view`, result bounds | `fetch.content` |

</target_contract>

<evaluation_units>

Every predicate is evaluated against a target-specific unit. The planner must
preserve this unit when it pushes down, materializes, or filters results.

| Target | Evaluation unit | `where` omitted |
|---|---|---|
| `code` | a concrete code match occurrence | invalid |
| `content` | a bounded content block/file read | allowed; fetch-only |
| `structure` | a tree entry or directory slice | allowed; tree-only |
| `files` | a file or directory entry | allowed; list files in scope |

Rules:

- On `target:"code"`, text/regex/structural predicates return match rows.
- On `target:"files"`, text/regex/structural predicates mean the file contains
  at least one matching occurrence. `not` over those predicates means the file
  contains no such occurrence.
- Negative file queries require a complete candidate universe. For provider
  sources, that usually means materialization or a diagnostic.
- `where` omission is not a wildcard for `target:"code"`. Agents must provide a
  real code predicate.

</evaluation_units>

<top_level_bounds>

| Field | Meaning |
|---|---|
| `limit` | maximum logical result count before or with pagination |
| `page` | result page number for result rows/files/entries |
| `itemsPerPage` | result rows/files/entries per page |
| `explain` | include normalized query, defaults, routing, diagnostics, budgets, and backend plan |

</top_level_bounds>

<pagination_domains>

| Domain | Fields | Continuation | Use |
|---|---|---|---|
| result rows | top-level `page`, `itemsPerPage`, `limit` | `next.page` | files, matches, tree entries, packages, repos, PR rows |
| matches within one file | `controls.search.matchPage`, `maxMatchesPerFile` | `next.matchPage` | noisy files with many matches |
| content windows | `fetch.content.charOffset`, `charLength` | `next.charRange` | large files, PR bodies, patches, binary strings |
| artifact entries | V2 artifact page fields | `next.artifactEntries` | archive listing |
| semantic rows | V2 semantic page fields | `next.semantic` | LSP symbol/location/call results |

Rules:

- A response may expose more than one continuation, but only one primary
  pagination domain is reflected in top-level `pagination`.
- Agents must follow the named continuation rather than computing offsets,
  match pages, or artifact pages.
- `combine:"merge"` is invalid across children with different primary
  pagination domains.
- Apply `limit` before rendering and pagination for the primary result-row
  domain.
- Apply `itemsPerPage` to the already-limited primary result-row domain.
- Apply `maxMatchesPerFile` and `matchPage` only inside one result row/file.
- Apply `charOffset` and `charLength` only inside content-bearing rows or
  continuations.

</pagination_domains>

<select_contract>

`select` is an array of result field names and continuation names. It projects
output only; it must not cause hidden unbounded fetches.

| Field | Use |
|---|---|
| `repo` | GitHub repository identity |
| `localPath` | materialized local path |
| `path` | file or tree path |
| `line` | start line for code/content evidence |
| `endLine` | end line when a result spans lines |
| `column` | start column when available |
| `snippet` | bounded code/search snippet |
| `content` | bounded content body for `target:"content"` |
| `metavars` | structural captures |
| `size` | file/tree size when available |
| `modified` | file modification time when available |
| `pagination` | pagination state for the active result domain |
| `diagnostics` | result-local diagnostics |
| `next.*` | executable continuation handles |

Rules:

- Unknown select fields produce `unknownField` or `unsupportedPredicate`.
- `select:["content"]` is valid only for bounded content results.
- Search rows should select `next.fetch` instead of full content.

</select_contract>

</section>

<section id="source-and-scope">

## Source And Scope

<source_schema>

```ts
type QuerySource =
  | { kind: "local"; path: string }
  | { kind: "github"; repo?: string; owner?: string; ref?: string }
  | { kind: "materialized"; localPath: string; source?: QuerySource }
```

</source_schema>

<source_params>

| Source field | Required when | Meaning |
|---|---|---|
| `kind` | always | source discriminator |
| `path` | local source | local file or directory root |
| `repo` | optional GitHub source | canonical `owner/name` repository id |
| `owner` | optional GitHub source | GitHub user/org scope for provider search; in raw input only, may combine with repository name sugar |
| `ref` | optional GitHub source | branch, tag, or commit |
| `localPath` | materialized source | local path returned by materialization |
| `source` | optional materialized source | original provider source |

</source_params>

<scope_schema>

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

</scope_schema>

<scope_params>

| Scope field | Meaning |
|---|---|
| `path` | source-relative traversal root or provider path prefix |
| `language` | canonical language filter; normalizes CLI `--type` and raw `langType` |
| `include` | include globs for local/materialized search |
| `exclude` | exclude globs when backend supports them |
| `excludeDir` | directory names to skip entirely |
| `hidden` | include hidden files when backend supports it |
| `noIgnore` | ignore `.gitignore`/ignore files when backend supports it |
| `maxDepth` | tree/file traversal depth |

</scope_params>

<rules>

- `from` identifies the corpus.
- Repository subpaths belong in `scope.path`.
- Local paths must pass existing path validation.
- Canonical GitHub repository identity is `repo:"owner/name"`.
- Raw input may use `{owner:"org", repo:"name"}`; normalization rewrites it to
  `repo:"org/name"`.
- GitHub `owner` without `repo` scopes code/repository search to one owner.
- GitHub `from:{kind:"github"}` with neither `owner` nor `repo` means provider
  search across GitHub. It is valid only for provider-search targets and must
  never be materialized.
- GitHub content, structure, and materialization require a concrete repository.
- GitHub `ref` is branch, tag, or commit.
- OQL uses canonical `language`; CLI `--type` and raw `langType` normalize to
  it.
- Path and language constraints are scope, not `controls`.
- `scope.path` narrows the traversal root or provider prefix. It is not the
  proof language for path matching.
- Use `where:{kind:"field",field:"path",op:"glob"}` or
  `where:{kind:"field",field:"path",op:"regex"}` when exact path intent matters.
- Glob semantics are local/materialized by default. Provider-native path
  qualifiers that are only prefix filters must be reported as approximate unless
  a local/materialized residual check proves them.
- Unsupported scope fields become diagnostics unless the planner can route to a
  local/materialized lane.
- For V1 structural search, current local execution supports `include`,
  `excludeDir`, language-derived includes, and `maxFiles`; other scope fields
  must be diagnostic or residual-planned before they affect proof.

</rules>

</section>

<section id="predicates">

## Predicates

<attention priority="high">

`where` is a discriminated union. Every predicate leaf must have `kind`.

</attention>

<predicate_union>

```ts
type Predicate =
  | { kind: "all"; id?: PredicateId; of: Predicate[] }
  | { kind: "any"; id?: PredicateId; of: Predicate[] }
  | { kind: "not"; id?: PredicateId; predicate: Predicate }
  | TextPredicate
  | RegexPredicate
  | StructuralPredicate
  | FieldPredicate

type PredicateId = string
```

</predicate_union>

<boolean_predicates>

| Predicate | Fields | Meaning |
|---|---|---|
| `all` | `of: Predicate[]` | every child predicate must match |
| `any` | `of: Predicate[]` | at least one child predicate must match |
| `not` | `predicate: Predicate` | child predicate must not match |

Rules:

- Empty `all.of` or `any.of` is invalid.
- `not` must contain exactly one child predicate.
- Predicate IDs are optional input. The compiler must assign stable IDs to every
  predicate node before planning so `--explain`, diagnostics, and provenance can
  refer to exact nodes.
- Canonical boolean nodes are only `all`, `any`, and `not`.
- Providers may not support all boolean forms; unsupported forms become
  diagnostics or route to materialized/local execution.

</boolean_predicates>

<boolean_sugar>

Accepted sugar must normalize away before planning:

| Sugar | Canonical rewrite |
|---|---|
| `and` | `all` |
| `or` | `any` |
| `noneOf:[A,B]` | `not(any(A,B))` |
| `xor:[A,B]` | `any(all(A,not(B)), all(not(A),B))` |
| `oneOf:[A,B,...]` | exactly-one expansion, or `budgetExhausted` if expansion exceeds `controls.budget.maxBooleanExpansion` |

Rules:

- `xor` is not a backend feature.
- `xor` is binary in V1. Multi-way exclusive matching must use `oneOf`.
- `oneOf` expansion must stop before it exceeds `maxBooleanExpansion`.
- `xor` over provider sources requires a complete candidate universe or
  materialization, because the planner must prove both positive and negative
  branches.
- Boolean normalization should flatten nested `all`/`any`, remove double
  negation, and apply De Morgan rewrites only when they do not change the
  evaluation unit or proof strength.

</boolean_sugar>

<residual_logic_rules>

| Boolean shape | Safe routing rule |
|---|---|
| `all(PUSHDOWN, RESIDUAL)` | push down the supported predicates, then residual-filter candidates |
| `any(PUSHDOWN, RESIDUAL)` | requires union coverage; if the residual branch cannot enumerate candidates, materialize or fail |
| `not(P)` | exact only when the planner has the full evaluation universe for the target |
| `xor(A,B)` sugar | exact only when both branches and their negations are exact over the same universe |

Rules:

- Boolean trees must not be flattened into one global pushed/residual/routed
  list if that loses parent/child semantics.
- `--explain` must show routing per predicate node, not only per backend call.
- A residual predicate under `not`, `any`, or `xor` cannot produce `proof` unless
  the candidate universe is complete.

</residual_logic_rules>

<predicate kind="text">

### Text

```ts
interface TextPredicate {
  id?: PredicateId
  kind: "text"
  value: string
  case?: "smart" | "sensitive" | "insensitive"
  wholeWord?: boolean
}
```

<text_params>

| Field | Meaning |
|---|---|
| `kind:"text"` | literal text predicate |
| `value` | exact text to search for |
| `case` | case behavior; `smart` follows current local ripgrep behavior |
| `wholeWord` | match whole words only when backend supports it |

</text_params>

Rules:

- `text` means literal text.
- Local compilation sets `fixedString:true`.
- Default `case` is `smart`, matching current local ripgrep behavior.
- Provider text search may not support exact local case behavior; unsupported
  parts must appear in `diagnostics`.

</predicate>

<predicate kind="regex">

### Regex

```ts
interface RegexPredicate {
  id?: PredicateId
  kind: "regex"
  value: string
  dialect?: "rust" | "pcre2" | "provider"
  case?: "smart" | "sensitive" | "insensitive"
  wholeWord?: boolean
  multiline?: boolean
  dotAll?: boolean
}
```

<regex_params>

| Field | Meaning |
|---|---|
| `kind:"regex"` | regular-expression predicate |
| `value` | regex pattern string |
| `dialect` | `rust` for local default, `pcre2` for advanced local regex, `provider` for provider-native search |
| `case` | case behavior; unsupported provider behavior must be diagnosed |
| `wholeWord` | wrap or lower to whole-word matching when backend supports it |
| `multiline` | allow matching across lines when backend supports it |
| `dotAll` | make `.` match newlines when backend supports it |

</regex_params>

Rules:

- Default local dialect is `rust`.
- `pcre2` requires local or materialized execution.
- Lookaround and backreferences require `dialect:"pcre2"`.
- Provider regex support is provider-specific and must be explained.

</predicate>

<predicate kind="structural">

### Structural

```ts
interface StructuralPredicate {
  id?: PredicateId
  kind: "structural"
  lang: string
  pattern?: string
  rule?: StructuralRule
}

interface StructuralRule {
  pattern?: string
  kind?: string
  inside?: StructuralRule
  has?: StructuralRule
  not?: StructuralRule
  all?: StructuralRule[]
  any?: StructuralRule[]
  stopBy?: "end"
}
```

<structural_params>

| Field | Meaning |
|---|---|
| `kind:"structural"` | AST/tree-sitter predicate |
| `lang` | required parser language; use canonical language ids |
| `pattern` | code-shaped structural pattern |
| `rule` | JSON structural rule for relational matching |
| `rule.pattern` | nested code-shaped structural pattern |
| `rule.kind` | AST node kind constraint |
| `rule.inside` | ancestor/containing rule |
| `rule.has` | descendant/subtree rule |
| `rule.not` | negated nested rule |
| `rule.all` | every nested rule must match |
| `rule.any` | at least one nested rule must match |
| `rule.stopBy` | relational traversal boundary; use `"end"` for V1 relational rules |

</structural_params>

<structural_subset>

| Structural feature | V1 status | Notes |
|---|---|---|
| `pattern` | active | code-shaped tree-sitter pattern |
| `$X` | active | one AST node capture |
| `$$$ARGS` / `$$$NAME` | active | node-list capture |
| `rule.pattern` | active | nested code-shaped pattern |
| `rule.kind` | active | AST node kind constraint |
| `rule.inside` | active | containing/ancestor relation |
| `rule.has` | active | descendant/subtree relation |
| `rule.not` | active | structural negation inside a rule |
| `rule.all` / `rule.any` | active | structural rule composition |
| `rule.stopBy:"end"` | active | required for bounded relational intent |

</structural_subset>

<structural_deferred>

| Feature | V1 handling |
|---|---|
| `precedes`, `follows`, `field`, `regex`, `nthChild`, `range`, `matches` | `unsupportedPredicate` unless the engine proves support later |
| reusable named structural refs | V2 |
| metavariable constraints and focus range | V2 |
| rule ids, messages, severities, tests | V2 |
| `fix`, `transform`, codemod output | V3 dry-run only |
| Semgrep or ast-grep compatibility mode | not V1 |

</structural_deferred>

Rules:

- Exactly one of `pattern` or `rule` is required.
- `lang` is required.
- `$X` captures one AST node.
- `$$$ARGS` captures a node list.
- Bare `$$$` is an advanced document-root probe, not normal agent syntax.
- Patterns must parse as complete source nodes. For example, a TypeScript class
  usually needs `class $NAME { $$$BODY }`, not `class $NAME`.
- The OQL compiler may serialize `rule` to the current engine's YAML rule
  string, but agents should use the JSON object form.
- OQL structural rules are Octocode tree-sitter rules. They are not full
  ast-grep or Semgrep compatibility.
- Structural search is local-only. GitHub structural queries need
  `materialize.mode:"auto"` or `"required"`.
- Rule ids, messages, severities, tests, reusable refs, and fixes are not V1.

</predicate>

<predicate kind="field">

### Field

```ts
interface FieldPredicate {
  id?: PredicateId
  kind: "field"
  field:
    | "path"
    | "basename"
    | "extension"
    | "size"
    | "modified"
    | "entryType"
  op:
    | "="
    | "!="
    | "in"
    | "exists"
    | "glob"
    | "regex"
    | ">"
    | ">="
    | "<"
    | "<="
    | "within"
  value?: unknown
}
```

<field_params>

| Field | Meaning |
|---|---|
| `kind:"field"` | file/result attribute predicate |
| `field` | file/result attribute to test |
| `op` | comparison operator |
| `value` | comparison value; omitted only for `op:"exists"` |

</field_params>

<field_operator_semantics>

| Operator | Meaning |
|---|---|
| `=` / `!=` | equality or inequality |
| `in` | field value is one of the provided values |
| `exists` | field is present |
| `glob` | glob comparison, mainly for path-like fields |
| `regex` | regex comparison, mainly for path-like fields |
| `>` / `>=` / `<` / `<=` | numeric or timestamp comparison |
| `within` | timestamp or size range comparison |

</field_operator_semantics>

<field_value_rules>

| Operator | Required value shape |
|---|---|
| `=` / `!=` | scalar string, number, or boolean matching the field type |
| `in` | non-empty array of scalar values matching the field type |
| `exists` | no `value` |
| `glob` | glob string; path separators normalize to `/` |
| `regex` | regex string; Rust regex unless the planner routes to PCRE2-capable local proof |
| `>` / `>=` / `<` / `<=` | number, size string, duration string, or ISO timestamp according to field type |
| `within` | `{ "from"?: value, "to"?: value }` or a duration string such as `"7d"` |

Rules:

- Size strings use `/^[0-9]+(b|k|m|g)$/`, base 1024, lowercase units.
- Duration strings use `/^[0-9]+(m|h|d|w)$/`.
- Timestamps are ISO 8601 strings.
- `within` for `modified` accepts a duration string or `{from,to}` ISO range.
- `within` for `size` accepts `{from,to}` size/number range.
- `extension` values normalize without a leading dot.
- `entryType` values are `"file"` or `"directory"` in OQL. Backends may lower
  them to their native forms such as `f` and `d`.
- `size`, `modified`, and `entryType` are valid for `target:"files"` and
  `target:"structure"`; they are result attributes for `target:"code"` only
  after the backend can prove them exactly.
- Type mismatches produce `fieldTypeMismatch`.

</field_value_rules>

Rules:

- Field predicates are logical predicates, not output controls.
- `path`, `basename`, and `extension` may push down to GitHub when supported.
- `size`, `modified`, and `entryType` may require local/materialized execution
  depending on source and target.

</predicate>

</section>

<section id="materialization">

## Materialization

<materialize_schema>

```ts
interface MaterializePolicy {
  mode: "never" | "auto" | "required"
  strategy?: "file" | "tree" | "subtree" | "repo"
  allowFullRepo?: boolean
  forceRefresh?: boolean
}
```

</materialize_schema>

<materialize_params>

| Field | Meaning |
|---|---|
| `mode:"never"` | provider-only; fail if local proof is required |
| `mode:"auto"` | planner may materialize bounded source if needed |
| `mode:"required"` | planner must materialize first or fail |
| `strategy:"file"` | fetch/cache one file |
| `strategy:"tree"` | fetch/cache tree info |
| `strategy:"subtree"` | clone/fetch bounded subtree |
| `strategy:"repo"` | full repo, only when explicitly allowed and bounded |
| `allowFullRepo` | explicit permission for `strategy:"repo"` |
| `forceRefresh` | bypass stale cache when supported |

</materialize_params>

<defaults>

- Local source: no materialization needed.
- GitHub source: `mode:"never"` unless the query explicitly sets otherwise.

</defaults>

<rules>

- `never`: provider-only. Local-only predicates produce
  `requiresMaterialization`.
- `auto`: planner may materialize a bounded repo/path/ref when needed.
- `required`: planner must materialize first or fail.
- Materialization requires bounded `repo`, `ref` when available, and `scope.path`
  unless the user explicitly allows a full repo.
- `strategy:"repo"` requires `allowFullRepo:true`,
  `controls.budget.maxMaterializedBytes`, and a provider/tree preflight that can
  reject oversized repositories before clone/fetch.
- Materialized results must return `localPath`, original source, ref/cache
  information, and executable local follow-ups.

</rules>

<remote_as_local_flow>

One OQL query may plan a multi-step research flow:

1. Use provider search/tree/content to bound the candidate corpus.
2. Materialize the bounded file, subtree, or repository.
3. Run V1 local proof tools over the materialized path: ripgrep, PCRE2,
   structural AST, exact content fetch, and symbol-outline minification.
4. Return one result envelope with provenance for every step and continuations
   for exact reads, next pages, V2 semantic follow-ups, or refresh.

This is the OQL form of current `grep --repo`: remote input, local proof.

Rules:

- The planner must not materialize broad GitHub/org/global scopes.
- `strategy:"repo"` requires `allowFullRepo:true`; otherwise repair to
  `strategy:"subtree"` with a concrete `scope.path`.
- Current backing for GitHub materialization is `ghCloneRepo` with `owner`,
  `repo`, `branch`, `sparsePath`, and `forceRefresh`. Unsupported strategy or
  budget fields must produce diagnostics, not silent ignore.
- Provider zero results are not proof when the query asked for a local-only
  predicate such as PCRE2, structural AST, exact absence, or semantic proof.

</remote_as_local_flow>

</section>

<section id="fetch-and-controls">

## Fetch And Controls

<fetch_schema>

```ts
interface FetchInstructions {
  content?: {
    range?: { startLine?: number; endLine?: number; contextLines?: number }
    match?: { text: string; regex?: boolean; caseSensitive?: boolean }
    contentView?: "exact" | "compact" | "symbols"
    charOffset?: number
    charLength?: number
    fullContent?: boolean
  }
  tree?: {
    maxDepth?: number
    includeSizes?: boolean
  }
}
```

</fetch_schema>

<fetch_params>

| Field | Meaning |
|---|---|
| `content.range.startLine` | first 1-based line to read |
| `content.range.endLine` | last 1-based line to read |
| `content.range.contextLines` | context lines around a match/range |
| `content.match.text` | content-local match anchor |
| `content.match.regex` | treat `match.text` as regex |
| `content.match.caseSensitive` | match case exactly |
| `content.contentView` | `exact`, `compact`, or `symbols` |
| `content.charOffset` | character offset for content pagination |
| `content.charLength` | maximum characters to return |
| `content.fullContent` | request full file only when bounded and allowed |
| `tree.maxDepth` | tree traversal depth |
| `tree.includeSizes` | include size info when backend supports it |

</fetch_params>

<controls_schema>

```ts
interface QueryControls {
  search?: {
    countLinesPerFile?: boolean
    countMatchesPerFile?: boolean
    onlyMatching?: boolean
    unique?: boolean
    countUnique?: boolean
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

</controls_schema>

<controls_params>

| Field | Meaning |
|---|---|
| `search.countLinesPerFile` | count matching lines per file |
| `search.countMatchesPerFile` | count total matches per file |
| `search.onlyMatching` | return matched substrings only |
| `search.unique` | unique matched substrings per file; requires `onlyMatching` |
| `search.countUnique` | unique matched substrings with counts; requires `onlyMatching` |
| `search.matchWindow` | characters around an `onlyMatching` hit |
| `search.matchContentLength` | maximum snippet length |
| `search.maxMatchesPerFile` | per-file match page size/cap |
| `search.matchPage` | page within one file's matches |
| `search.sort` | local result ordering |
| `search.sortReverse` | reverse supported sort order |
| `search.rankingProfile` | relevance profile |
| `search.debugRanking` | include ranking reasons |
| `budget.maxFiles` | maximum files to inspect or return |
| `budget.maxCandidates` | maximum provider/local candidates before residual filtering |
| `budget.maxBytes` | maximum bytes to inspect/read |
| `budget.maxMaterializedBytes` | maximum external bytes to materialize |
| `budget.maxPlanNodes` | maximum predicate/explain plan nodes after normalization |
| `budget.maxBooleanExpansion` | maximum generated predicate nodes from boolean sugar |
| `budget.timeoutMs` | execution time budget |

</controls_params>

<rules>

- `fetch` acquires content or tree data.
- `controls` affects cost, pagination domains, snippets, ranking, and debug
  output.
- `controls` must not change what logically matches.
- Legacy `filesWithoutMatch` input is not canonical OQL. Normalize it to
  `target:"files"` plus `where:{kind:"not",predicate:...}`.
- `onlyMatching` is the safe way to enumerate values from minified one-line
  files.
- `matchPage` pages matches inside one file; top-level `page` pages result
  files/rows.
- Discovery/count modes still require exact fetch before quoting or patching.

</rules>

<content_view_mapping>

| OQL | Current backing | Use |
|---|---|---|
| `exact` | `minify:"none"` | quotes, patches, diffs |
| `compact` | `minify:"standard"` | normal reading |
| `symbols` | `minify:"symbols"` | cheap symbol/signature outline for orientation |

Rules:

- `fetch.content.contentView:"symbols"` is syntax/signature extraction from
  content. It is not LSP semantic proof.
- If symbol extraction falls back or is unsupported for a language, return
  `signatureUnsupported` or `partialResult` diagnostics and a content-fetch
  continuation.

</content_view_mapping>

</section>

<section id="defaults">

## Defaults

<attention priority="high">

`octocode search --explain` must show all applied defaults.

</attention>

| Field | V1 default |
|---|---|
| `schema` | `oql/v1` |
| `view` | `paginated` |
| `page` | `1` |
| `itemsPerPage` | `25` for search rows unless a target-specific cap is lower |
| `materialize.mode` for GitHub | `never` |
| `text.case` | `smart` |
| `regex.dialect` local | `rust` |
| `regex.case` | `smart` |
| `fetch.content.contentView` | `compact` |
| `fetch.content.charLength` | `20000` when content pagination is needed |
| `controls.search.matchContentLength` | `500` |
| `controls.budget.maxPlanNodes` | `128` |
| `controls.budget.maxBooleanExpansion` | `64` generated predicate nodes |
| normal code context | `2` lines |
| detailed code context | `3` lines |
| local search sort | `relevance` |
| local ranking profile | `auto` |
| structural file cap | engine cap unless `controls.budget.maxFiles` is lower |
| structural per-file byte cap | engine cap |

</section>

<section id="normalization">

## Normalization

<normalization_rule>

Input sugar is accepted only if it has a deterministic rewrite. Ambiguous sugar
must fail with a repair diagnostic. Canonical output from `--explain` must not
contain shorthand fields.

</normalization_rule>

<example kind="sugar">

```jsonc
{
  "repo": "facebook/react",
  "path": "packages/react",
  "pattern": "useEffect($$$ARGS)",
  "lang": "js"
}
```

</example>

<example kind="canonical">

```jsonc
{
  "schema": "oql/v1",
  "target": "code",
  "from": { "kind": "github", "repo": "facebook/react" },
  "scope": { "path": "packages/react" },
  "where": {
    "kind": "structural",
    "lang": "js",
    "pattern": "useEffect($$$ARGS)"
  },
  "materialize": { "mode": "auto", "strategy": "subtree" },
  "view": "paginated"
}
```

</example>

<rewrite_table>

| Sugar | Canonical |
|---|---|
| top-level `repo` | `from:{kind:"github",repo}` |
| top-level `owner` + `repo` | `from:{kind:"github",owner,repo}` then normalize repo identity |
| top-level local `path` with no repo | `from:{kind:"local",path}` unless also used as `scope.path` |
| GitHub `path` | `scope.path` |
| CLI `--type` or raw `langType` | `scope.language` or structural `lang` by context |
| top-level `text` | `where:{kind:"text",value}` |
| top-level `regex` | `where:{kind:"regex",value}` |
| top-level `pattern` + `lang` | `where:{kind:"structural",lang,pattern}` |
| top-level `rule` + `lang` | `where:{kind:"structural",lang,rule}` |
| `minify` | `fetch.content.contentView` |
| `and` / `or` | `all` / `any` |
| `xor` | canonical `any(all(A,not(B)),all(not(A),B))` expansion |
| text/regex `invert:true` | wrap predicate in `where:{kind:"not",predicate:...}` |
| legacy `filesOnly:true` | `view:"discovery"` plus `select:["path","next.fetch"]` unless a stricter `select` is present |
| legacy `filesWithoutMatch` | `target:"files"` + `where:{kind:"not",predicate:...}` |

</rewrite_table>

<path_sugar_resolution>

Rules:

- If raw input has `repo`, top-level `path` means `scope.path`.
- If raw input has no `repo` and no explicit `from`, top-level `path` means
  `from:{kind:"local",path}`.
- If raw input has explicit `from` and top-level `path`, top-level `path` means
  `scope.path`.
- If raw input has both `scope.path` and top-level `path`, it is ambiguous and
  must fail with `ambiguousSugar`.

</path_sugar_resolution>

</section>

<section id="planner">

## Planner

<planner_modes>

| Mode | Meaning |
|---|---|
| `PUSHDOWN` | backend can evaluate it directly |
| `RESIDUAL` | fetch bounded candidates and filter locally |
| `ROUTE` | move to another lane, usually materialization |
| `UNSUPPORTED` | fail with diagnostics and repair hints |

</planner_modes>

<invariant>

```text
pushed predicates + residual predicates + routed predicates == all predicates
```

</invariant>

<explain_output>

`octocode search --explain` must return:

- original input
- normalized canonical query
- applied defaults
- predicate routing
- selected backend calls
- materialization decision
- residual filters
- effective budgets
- diagnostics
- executable continuations

</explain_output>

<explain_schema>

```ts
interface OqlExplainPlan {
  input: unknown
  normalized: OqlCanonicalInputV1
  defaults: Record<string, unknown>
  nodes: Array<{
    predicateId: PredicateId
    path: string
    route: "PUSHDOWN" | "RESIDUAL" | "ROUTE" | "UNSUPPORTED"
    backend?: string
    reason: string
  }>
  backendCalls: Array<{
    backend: string
    source: QuerySource
    operation: string
    exact: boolean
  }>
  materialization?: MaterializePolicy & {
    required: boolean
    reason: string
  }
  budgets: QueryControls["budget"]
  truncated?: boolean
  diagnostics: OqlDiagnostic[]
  next?: Record<string, OqlContinuation>
}
```

</explain_schema>

<explain_limits>

- `nodes.length` must not exceed `controls.budget.maxPlanNodes`.
- If an explain plan is truncated, set `truncated:true`, add `planTruncated`,
  and include enough parent nodes to explain why proof was or was not possible.
- Truncating explain output must not truncate execution semantics. If execution
  itself cannot proceed within budget, use `budgetExhausted`.

</explain_limits>

</section>

<section id="backend-mapping">

## Backend Mapping

| Canonical query | Current compilation |
|---|---|
| local `target:"code"` + `text` | `localSearchCode keywords + fixedString:true` |
| local `target:"code"` + `regex.dialect:"rust"` | `localSearchCode keywords` |
| local `target:"code"` + `regex.dialect:"pcre2"` | `localSearchCode perlRegex:true` |
| local `target:"code"` + `structural` | `localSearchCode mode:"structural"` |
| local `target:"files"` + `not(text/regex/structural)` | `localSearchCode filesWithoutMatch` or local candidate enumeration plus residual proof |
| local `target:"content"` | `localGetFileContent` |
| local `target:"structure"` | `localViewStructure` |
| local `target:"files"` | `localFindFiles` |
| GitHub `target:"code"` provider-capable predicates | `ghSearchCode` |
| GitHub owner-wide or provider-wide `target:"code"` | `ghSearchCode` with owner/repo omitted as requested |
| GitHub `target:"content"` | `ghGetFileContent` |
| GitHub `target:"structure"` | `ghViewRepoStructure` |
| GitHub local-only predicate + `materialize.mode:"auto"` | `ghCloneRepo` then local tool |

<provider_capability_examples>

| Query shape | Required outcome |
|---|---|
| GitHub regex with PCRE2-only features and `materialize.mode:"never"` | `unsupportedPredicate` or `requiresMaterialization` |
| GitHub structural with `materialize.mode:"auto"` | bounded clone/subtree, then local structural search |
| GitHub structural with `materialize.mode:"never"` | no weaker provider text search; return diagnostic |
| GitHub path prefix search | provider candidate/proof only for prefix semantics |
| GitHub path glob/regex proof | materialize or residual-check candidates |
| provider zero results with all predicates pushed exactly | may be `proof` of absence |
| provider zero results with residual/local-only predicates | not proof; return materialization continuation or diagnostic |
| provider symbol search in V2 | definitions only unless LSP proves references/call hierarchy |

</provider_capability_examples>

</section>

<section id="result-envelope">

## Result Envelope

<result_schema>

```ts
type OqlResultRow =
  | OqlCodeResultRow
  | OqlFileResultRow
  | OqlTreeResultRow
  | OqlContentResultRow

interface OqlCodeResultRow {
  kind: "code"
  source: QuerySource
  path: string
  line: number
  endLine?: number
  column?: number
  snippet?: string
  metavars?: Record<string, unknown>
  next?: Record<string, OqlContinuation>
}

interface OqlFileResultRow {
  kind: "file"
  source: QuerySource
  path: string
  entryType: "file" | "directory"
  size?: number
  modified?: string
  next?: Record<string, OqlContinuation>
}

interface OqlTreeResultRow {
  kind: "tree"
  source: QuerySource
  path: string
  entryType: "file" | "directory"
  depth: number
  size?: number
  children?: OqlTreeResultRow[]
  next?: Record<string, OqlContinuation>
}

interface OqlContentResultRow {
  kind: "content"
  source: QuerySource
  path: string
  content: string
  range?: { startLine?: number; endLine?: number; charOffset?: number; charLength?: number }
  contentView: "exact" | "compact" | "symbols"
  next?: Record<string, OqlContinuation>
}

interface OqlResultEnvelope {
  queryId?: string
  queryIndex?: number
  results: OqlResultRow[]
  pagination?: Pagination
  next?: Record<string, OqlContinuation>
  diagnostics: OqlDiagnostic[]
  provenance: OqlProvenance[]
  evidence: {
    answerReady: boolean
    complete: boolean
    kind: "proof" | "partial" | "candidate" | "unsupported"
  }
}

interface OqlBatchResultEnvelope {
  batchId?: string
  mode: "independent" | "merge"
  children: Array<{
    queryId: string
    queryIndex: number
    envelope: OqlResultEnvelope
  }>
  merged?: OqlResultEnvelope
  diagnostics: OqlDiagnostic[]
}
```

</result_schema>

<result_params>

| Field | Meaning |
|---|---|
| `queryId` | stable query id for this result envelope |
| `queryIndex` | zero-based index in the batch when applicable |
| `results` | result rows, files, tree entries, or content blocks depending on `target` |
| `pagination` | active page state for the primary result domain |
| `next` | named executable continuations |
| `diagnostics` | query-level warnings/errors and repair hints |
| `provenance` | backend/source/predicate routing evidence |
| `evidence.answerReady` | true only when the result is strong enough to answer |
| `evidence.complete` | true only when no relevant pages, residual checks, or diagnostics remain |
| `evidence.kind` | proof strength: `proof`, `partial`, `candidate`, or `unsupported` |

</result_params>

<batch_result_params>

| Field | Meaning |
|---|---|
| `batchId` | optional id for the batch request |
| `mode` | `independent` or `merge` |
| `children` | one result envelope per child query, always preserved |
| `children[].queryId` | stable query id; compiler assigns one when omitted |
| `children[].queryIndex` | zero-based child query index |
| `merged` | optional merged envelope when `combine:"merge"` succeeds |
| `diagnostics` | batch-level diagnostics such as merge incompatibility |

</batch_result_params>

<proof_lattice>

| Evidence kind | Meaning |
|---|---|
| `proof` | every required predicate was evaluated exactly over the required universe |
| `partial` | some pages, candidates, files, or residual checks remain |
| `candidate` | useful lead, but at least one predicate or provider filter is approximate |
| `unsupported` | planner could not execute the requested semantics |

Rules:

- `answerReady:true` requires `evidence.kind:"proof"` and
  `evidence.complete:true`, unless the user asked only for candidates.
- Per-result proof cannot upgrade whole-query proof if pagination, truncation,
  residual predicates, or unsupported boolean branches remain.
- Diagnostics that block proof must set `blocksAnswer:true`.
- A result with executable continuations is usually `partial` unless the
  continuation is optional enrichment.

</proof_lattice>

<pagination_schema>

```ts
interface Pagination {
  currentPage?: number
  totalPages?: number
  itemsPerPage?: number
  totalItems?: number
  hasMore: boolean
  next?: OqlContinuation
}
```

</pagination_schema>

<pagination_params>

| Field | Meaning |
|---|---|
| `currentPage` | current result page |
| `totalPages` | known total pages when backend can count |
| `itemsPerPage` | result rows per page |
| `totalItems` | known total item count when backend can count |
| `hasMore` | true when another page/window/match page exists |
| `next` | executable continuation for the next page/window |

</pagination_params>

<provenance_schema>

```ts
interface OqlProvenance {
  backend: string
  source: QuerySource
  predicateIds?: string[]
  pushed?: string[]
  residual?: string[]
  routed?: string[]
  materializedPath?: string
  cache?: "hit" | "miss" | "refresh" | "stale"
}
```

</provenance_schema>

<provenance_params>

| Field | Meaning |
|---|---|
| `backend` | concrete backend/tool lane used |
| `source` | source corpus used by that backend |
| `predicateIds` | predicate ids handled by this backend when ids exist |
| `pushed` | predicates evaluated directly by the backend |
| `residual` | predicates evaluated after candidate fetch/materialization |
| `routed` | predicates moved to another backend/lane |
| `materializedPath` | local path produced by remote-as-local materialization |
| `cache` | cache state for materialized/provider data |

</provenance_params>

<continuation_schema>

```ts
interface OqlContinuation {
  query: OqlCanonicalInputV1
  baseQueryId?: string
  queryIndex?: number
  why: string
  confidence: "exact" | "heuristic"
}
```

</continuation_schema>

<continuation_params>

| Field | Meaning |
|---|---|
| `query` | full normalized OQL query or batch to run next |
| `baseQueryId` | query id this continuation extends, when applicable |
| `queryIndex` | batch child index this continuation extends, when applicable |
| `why` | reason this continuation exists |
| `confidence` | `exact` when semantics are preserved, `heuristic` when it is a best-effort follow-up |

</continuation_params>

<continuation_rules>

- Continuations must be executable as-is.
- Continuations must not require clients to merge a partial patch into hidden
  state.
- If a continuation derives from a batch child, include `baseQueryId` and
  `queryIndex`.
- If a continuation changes proof strength, explain that in `why` and set
  `confidence:"heuristic"` unless semantics are exact.

</continuation_rules>

<continuation_names>

| Name | Use |
|---|---|
| `next.page` | next result page |
| `next.matchPage` | next match page inside a file |
| `next.charRange` | next content byte/char window |
| `next.fetch` | exact/compact content read |
| `next.structure` | tree follow-up |
| `next.search` | scoped code follow-up |
| `next.materialize` | bounded remote-as-local follow-up |
| `next.semantic` | V2 LSP follow-up from a file/line/symbol anchor |
| `next.packageSource` | V2 package-to-source-repository pivot |
| `next.pullRequestPage` | V2 PR body/file/comment/review/commit page |
| `next.commitPage` | V2 commit history page |
| `next.artifactEntries` | V2 archive entry page |
| `next.artifactStrings` | V2 binary string scan offset/page |
| `next.diff` | V2 diff or patch follow-up |

</continuation_names>

</section>

<section id="diagnostics">

## Diagnostics

<diagnostic_schema>

```ts
type DiagnosticCode =
  | "invalidQuery"
  | "ambiguousSugar"
  | "unknownField"
  | "unsupportedTarget"
  | "unsupportedPredicate"
  | "unsupportedBoolean"
  | "unsupportedScope"
  | "negativeUniverseRequired"
  | "residualNotExact"
  | "fieldTypeMismatch"
  | "requiresMaterialization"
  | "materializationNotAllowed"
  | "materializationFailed"
  | "providerUnindexed"
  | "providerSemanticsApproximate"
  | "partialResult"
  | "contentTruncated"
  | "matchTruncated"
  | "planTruncated"
  | "budgetExhausted"
  | "parserFailed"
  | "partialParse"
  | "signatureUnsupported"
  | "lspUnavailable"
  | "staleCache"
  | "sanitized"
  | "rateLimited"
  | "zeroMatches"

interface OqlDiagnostic {
  code: DiagnosticCode
  severity: "info" | "warning" | "error"
  queryPath?: string
  predicateId?: string
  backend?: string
  message: string
  blocksAnswer: boolean
  repair?: {
    message: string
    suggestedQuery?: OqlSearchInputV1
  }
  continuation?: OqlContinuation
}
```

</diagnostic_schema>

<diagnostic_codes>

| Code | Meaning |
|---|---|
| `invalidQuery` | schema or normalization failed |
| `ambiguousSugar` | input sugar has more than one possible canonical meaning |
| `unknownField` | query includes a field outside V1 |
| `unsupportedTarget` | target is not active in V1 |
| `unsupportedPredicate` | backend cannot evaluate predicate |
| `unsupportedBoolean` | boolean shape cannot be evaluated exactly |
| `unsupportedScope` | backend cannot honor scope exactly |
| `negativeUniverseRequired` | negative query requires a complete candidate universe |
| `residualNotExact` | residual filtering cannot preserve proof strength |
| `fieldTypeMismatch` | field predicate value does not match field/operator type |
| `requiresMaterialization` | local-only proof requested on provider source |
| `materializationNotAllowed` | query needs materialization but mode is `never` |
| `materializationFailed` | clone/fetch/cache failed |
| `providerUnindexed` | provider search may be incomplete |
| `providerSemanticsApproximate` | provider qualifier is candidate-grade, not exact proof |
| `partialResult` | more result pages or match pages exist |
| `contentTruncated` | content was cut by char/window budget |
| `matchTruncated` | matches were capped |
| `planTruncated` | explain-plan nodes were capped |
| `budgetExhausted` | query stopped at an explicit budget |
| `parserFailed` | structural parser failed for one or more files |
| `partialParse` | some files parsed, parser errors may hide matches |
| `signatureUnsupported` | symbol/signature extraction is unavailable or degraded |
| `lspUnavailable` | requested semantic operation needs LSP but no server/capability is available |
| `staleCache` | cached materialization may be stale |
| `sanitized` | secret/path sanitization changed output |
| `rateLimited` | provider rate limit blocked full execution |
| `zeroMatches` | query ran completely and found no matches |

</diagnostic_codes>

</section>

<section id="reserved-extension-targets">

## Reserved Extension Targets

<reserved_targets>

These target families are not valid V1 canonical targets. V1 must return
`unsupportedTarget` plus a repair continuation or current-tool hint. They are
reserved so V2/V3 extend the language without inventing another grammar.

| Target family | Backing surface | Required design rule |
|---|---|---|
| `semantics` | `lspGetSemantics`, `lsp` | position/capability based; do not treat plain symbol search as proof |
| `repositories` | `ghSearchRepos`, `repo` | typed GitHub filters, no string DSL dependency |
| `packages` | `npmSearch`, `pkg` | package-to-source-repo continuation |
| `pullRequests` | `ghHistoryResearch`, `pr` | page body/files/comments/reviews/commits independently |
| `commits` | `ghHistoryResearch`, `history` | path/date/branch scoped history and optional diff slices |
| `artifacts` | `localBinaryInspect`, `binary`, `unzip` | list/extract/decompress/strings/unpack with scan continuations |
| `diff` | `diff`, PR patch selectors | exact content ranges or selected patch hunks |
| `fixes` | future dry-run codemod | no mutation in V1/V2; V3 dry-run only |
| `dataflow` | future flow engine | candidate mode first, proof only with traces |

</reserved_targets>

<semantic_target_rules>

V2 semantic/LSP queries should use real anchors:

- `uri` or materialized local path;
- 1-based line and optional character/order hint;
- operation such as `definition`, `references`, `callers`, `callees`,
  `callHierarchy`, `hover`, `documentSymbols`, `typeDefinition`, or
  `implementation`;
- server capability and partial-result diagnostics.

Remote semantic queries must route through bounded materialization before LSP
runs. GitHub provider `symbol:`-style search can discover definitions, but it is
not reference, type, implementation, or call-hierarchy proof.

</semantic_target_rules>

</section>

<section id="examples">

## Examples

<example id="local-literal-search">

```jsonc
{
  "schema": "oql/v1",
  "target": "code",
  "from": { "kind": "local", "path": "./packages/octocode/src" },
  "scope": { "language": ["ts"] },
  "where": { "kind": "text", "value": "runCLI" },
  "select": ["path", "line", "snippet", "next.fetch"],
  "view": "paginated",
  "limit": 25
}
```

</example>

<example id="local-regex-pcre2">

```jsonc
{
  "schema": "oql/v1",
  "target": "code",
  "from": { "kind": "local", "path": "./src" },
  "where": {
    "kind": "regex",
    "value": "function\\s+(?=handle)",
    "dialect": "pcre2"
  },
  "view": "detailed"
}
```

</example>

<example id="local-structural-search">

```jsonc
{
  "schema": "oql/v1",
  "target": "code",
  "from": { "kind": "local", "path": "./src" },
  "where": {
    "kind": "structural",
    "lang": "ts",
    "pattern": "class $NAME { $$$BODY }"
  },
  "select": ["path", "line", "snippet", "metavars", "next.fetch"],
  "view": "detailed"
}
```

</example>

<example id="local-structural-rule">

```jsonc
{
  "schema": "oql/v1",
  "target": "code",
  "from": { "kind": "local", "path": "./src" },
  "where": {
    "kind": "structural",
    "lang": "ts",
    "rule": {
      "pattern": "await $X",
      "not": {
        "inside": {
          "kind": "try_statement",
          "stopBy": "end"
        }
      }
    }
  },
  "select": ["path", "line", "snippet", "metavars", "next.fetch"],
  "view": "detailed"
}
```

</example>

<example id="github-provider-search">

```jsonc
{
  "schema": "oql/v1",
  "target": "code",
  "from": { "kind": "github", "repo": "facebook/react", "ref": "main" },
  "scope": { "path": "packages/react", "language": ["js"] },
  "where": { "kind": "text", "value": "useEffect" },
  "materialize": { "mode": "never" },
  "select": ["repo", "path", "line", "snippet", "next.fetch"],
  "view": "paginated"
}
```

</example>

<example id="github-structural-materialized">

```jsonc
{
  "schema": "oql/v1",
  "target": "code",
  "from": { "kind": "github", "repo": "facebook/react", "ref": "main" },
  "scope": { "path": "packages/react", "language": ["js"] },
  "where": {
    "kind": "structural",
    "lang": "js",
    "pattern": "useEffect($$$ARGS)"
  },
  "materialize": { "mode": "auto", "strategy": "subtree" },
  "select": ["repo", "localPath", "path", "line", "snippet", "next.fetch"],
  "view": "detailed",
  "controls": {
    "budget": {
      "maxFiles": 500,
      "maxMaterializedBytes": 50000000,
      "timeoutMs": 30000
    }
  },
  "explain": true
}
```

</example>

<example id="content-fetch">

```jsonc
{
  "schema": "oql/v1",
  "target": "content",
  "from": { "kind": "local", "path": "./src/index.ts" },
  "fetch": {
    "content": {
      "range": { "startLine": 40, "endLine": 90 },
      "contentView": "exact"
    }
  },
  "select": ["path", "content", "next.search"]
}
```

</example>

<example id="structure">

```jsonc
{
  "schema": "oql/v1",
  "target": "structure",
  "from": { "kind": "github", "repo": "facebook/react", "ref": "main" },
  "scope": { "path": "packages/react" },
  "fetch": { "tree": { "maxDepth": 2, "includeSizes": true } },
  "view": "discovery"
}
```

</example>

<example id="files">

```jsonc
{
  "schema": "oql/v1",
  "target": "files",
  "from": { "kind": "local", "path": "./packages" },
  "scope": { "language": ["ts"], "excludeDir": ["node_modules", "dist"] },
  "where": {
    "kind": "field",
    "field": "basename",
    "op": "regex",
    "value": "^(index|main)\\.(ts|tsx)$"
  },
  "select": ["path", "size", "modified", "next.fetch"],
  "view": "discovery"
}
```

</example>

</section>

<section id="acceptance-gates">

## Acceptance Gates

<acceptance_gates>

1. Every example in this file parses as `oql/v1`.
2. Every sugar example normalizes to the documented canonical shape.
3. Unknown fields fail.
4. V2/V3 targets fail with `unsupportedTarget`.
5. Local text compiles to `localSearchCode fixedString:true`.
6. Local regex compiles to Rust regex or PCRE2 as requested.
7. Local structural compiles to current structural search.
8. GitHub structural with `materialize.mode:"never"` fails with
   `requiresMaterialization` or `materializationNotAllowed`.
9. GitHub structural with `materialize.mode:"auto"` routes through bounded clone
   and then local structural search.
10. Every partial result returns an executable OQL continuation.
11. `--explain` shows normalized query, defaults, routing, budgets, and
    diagnostics.
12. Existing path validation and secret sanitization remain in the execution
    path.
13. Bulk input preserves per-query diagnostics, provenance, evidence, and
    continuations.
14. Boolean sugar normalizes away before planning.
15. `not`/`xor` over provider candidates cannot return `proof` without a
    complete universe or materialized proof.
16. Legacy `invert` and `filesWithoutMatch` inputs normalize into canonical
    `where.not`.
17. `select` never changes result domains or triggers hidden fetches.
18. Legacy `filesOnly` normalizes to path-only projection and discovery view.
19. JSON structural rules lower to the engine YAML format through fixtures that
    cover `inside`, `has`, `not`, `all`, `any`, and `stopBy:"end"`.
20. `combine:"merge"` rejects incompatible rows with a repair diagnostic.
21. Explain-plan truncation emits `planTruncated` and never changes execution
    semantics.

</acceptance_gates>

</section>

<section id="future-work">

## Future Work

V2 adds LSP remote-as-local, repository/package/PR/history/binary/diff targets,
quick-command lowering, reusable structural rule refs, rule validation, and
budget controls beyond V1's safety caps.

V2 also adds richer structural language only when supported by the engine:
reusable rule refs, metavariable constraints, focus ranges, rule validation,
and resolved rule provenance. Do not claim Semgrep or ast-grep compatibility
unless a compatibility layer is actually implemented.

V3 adds dry-run fixes and dataflow:

- `target:"fixes"` returns proposed edits, ranges, replacement text, conflicts,
  and metavariable provenance; it does not mutate files.
- `target:"dataflow"` starts as candidate mode with `flowKind:"value"` or
  `"taint"`, `sources`, `sinks`, `sanitizers`, and `propagators`.
- Candidate flow must return `evidence.kind:"candidate"` and a diagnostic that
  prevents vulnerability/proof claims.
- Engine-backed flow proof requires traces, source availability, truncation
  state, dependency bounds, and provenance.
- Global/cross-package taint is allowed only after the backing engine can prove
  it with bounded dependencies.

</section>

</oql_system_prompt>
