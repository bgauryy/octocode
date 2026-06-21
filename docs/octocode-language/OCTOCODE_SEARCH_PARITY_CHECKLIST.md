# Octocode Search Parity Checklist

This document is the agent-facing audit guide for making `octocode search`
replace Octocode's raw tools and quick CLI commands without duplicating
implementation logic.

Authoritative contract:
https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_QUERY_LANGUAGE.md

Implementation plan:
https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_QUERY_LANGUAGE_PLAN.md

## Replacement Rule

`octocode search` may replace another Octocode surface only when all of these
are true:

1. The query lowers into a canonical OQL object and `--explain` shows the exact
   normalized query.
2. The plan calls the same backing runner in `packages/octocode-tools-core`.
3. Returned rows preserve the information an agent needs to decide, cite, and
   continue research.
4. Diagnostics make uncertainty explicit: partial result, approximate provider,
   unsupported target, truncation, auth/rate failures, stale cache, sanitizer.
5. Follow-ups are executable OQL continuations or explicitly marked legacy
   handoff data.
6. The CLI/MCP layer does not reimplement tool behavior, schema rules, routing,
   pagination, or output semantics.

Non-goal: `search` should not replace management/meta commands such as
`install`, `auth`, `login`, `logout`, `status`, `tools`, `context`, `--help`,
or `--version`.

## Ownership Boundary

| Layer | Owns | Must not own |
|---|---|---|
| `@octocodeai/octocode-core` | Public descriptions, schema text, command/tool guidance | Execution or interface-specific parsing |
| `packages/octocode-tools-core/src/oql` | OQL schema, normalization, shorthand lowering, planning, adapters, result envelope | CLI rendering or terminal argv concerns |
| `packages/octocode-tools-core/src/tools/*` | The existing 13 tool runners and security wrappers | OQL-specific presentation |
| `packages/octocode/src/cli` | argv parsing, local-vs-GitHub target classification, rendering | Search semantics, routing, backend field mapping |
| `packages/octocode-mcp` | MCP registration and transport | Tool behavior or OQL planning |

If a quick command needs a convenience form, put the reusable lowering helper in
`tools-core/oql` and call it from the CLI. The CLI may classify a string as a
local path or GitHub ref because that depends on filesystem/runtime context.

Evidence anchors:

- https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/oql/run.ts
- https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/oql/shorthand.ts
- https://github.com/bgauryy/octocode/blob/main/packages/octocode/src/cli/commands/search.ts
- https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/src/oql/adapters/v2.ts

## Raw Tool Parity Matrix

| Raw tool | OQL target | Required OQL shape | Replacement status | Agent checks |
|---|---|---|---|---|
| `localSearchCode` | `code` | `from:{kind:"local"}`, `where.kind:"text"|"regex"|"structural"` | Strong | Compare `path`, `line`, `snippet`, `metavars`, pagination, match truncation diagnostics. |
| `localGetFileContent` | `content` | `from:{kind:"local"}`, `fetch.content` | Strong | Verify `contentView`, line/char ranges, `contentTruncated`, exact-mode proof. |
| `localViewStructure` | `structure` | `from:{kind:"local"}`, `fetch.tree` | Strong | Verify file/dir entries, depth, sizes, pagination. |
| `localFindFiles` | `files` | `from:{kind:"local"}`, optional field/content predicates | Strong | Verify metadata filters, negative queries, local complete-universe semantics. |
| `localBinaryInspect` | `artifacts` | `from:{kind:"local",path}`, `params` passed to binary runner | Partial | Generic record rows must preserve mode-specific payloads and derived local paths. |
| `lspGetSemantics` | `semantics` | local/materialized/GitHub `from`, `params.type`, symbol/line fields | Partial | Must preserve locations/symbols/call rows, server diagnostics, remote materialization provenance. |
| `ghSearchCode` | `code` | `from:{kind:"github"}`, provider-safe predicate | Strong for provider search | Regex/provider semantics may be approximate; require materialization for AST/PCRE2/exact proof. |
| `ghGetFileContent` | `content` | `from:{kind:"github"}`, `scope.path`, `fetch.content` | Strong | Verify branch/ref, matchString/ranges, minification mode, char pagination. |
| `ghViewRepoStructure` | `structure` | `from:{kind:"github"}`, `scope.path`, `fetch.tree` | Strong | Verify repo/ref/path, depth, empty dirs, large tree truncation. |
| `ghCloneRepo` | materialization lane | `from:{kind:"github"}`, `materialize.mode`, bounded `scope.path` | Partial | Search uses clone internally; standalone clone/cache workflow still needs explicit materialization result/continuation. |
| `ghSearchRepos` | `repositories` | `target:"repositories"`, optional GitHub `from`, `params` | Partial | `params` is opaque; row payload is generic. Need typed schema docs and OQL continuations. |
| `ghHistoryResearch` | `pullRequests`, `commits`, `diff` | GitHub `from`, `params` for PR/commit/diff selectors | Partial | PR list/detail, merged state, patch selectors, comments/reviews must match raw tool. |
| `npmSearch` | `packages` | `target:"packages"`, `from:{kind:"npm"}` default, `params` | Partial | Package rows exist, but raw `data.next` is legacy handoff, not first-class OQL `next`. |

Status meanings:

- Strong: core runner path exists and row type is OQL-native.
- Partial: core runner path exists, but parity depends on generic `params`,
  generic `record` rows, renderer support, or missing OQL continuations.
- Not covered: target is intentionally unsupported or belongs outside search.

## CLI Command Parity Matrix

| CLI command | Should `search` replace it? | OQL mapping | Current parity check |
|---|---|---|---|
| `grep` | Yes | `target:"code"` | Text/regex/AST, discovery/detailed views, match paging, count/onlyMatching controls. |
| `cat` | Yes | `target:"content"` | Exact/compact/symbols views, line/char/match ranges. |
| `ls` | Yes | `target:"structure"` or `content` symbols | Tree browsing plus symbol outline through content `symbols` or future semantics. |
| `find` | Yes | `target:"files"` | Field predicates, metadata, content-contained queries, negative queries. |
| `lsp` | Yes | `target:"semantics"` | Local and remote-as-local LSP; must preserve lineHint and server diagnostics. |
| `repo` | Yes | `target:"repositories"` | Repo discovery rows, sorting/filter params, pagination. |
| `pkg` | Yes | `target:"packages"` | Package metadata, repository handoff, npm fallback diagnostics. |
| `pr` | Yes | `target:"pullRequests"` or `diff` | PR list/detail modes, comments/reviews/patch selectors. |
| `history` | Yes | `target:"commits"` or `pullRequests` | Commit history, path/subtree, PR handoff, rename/diff behavior. |
| `binary` | Yes | `target:"artifacts"` | Inspect/list/extract/decompress/strings/unpack modes and output paths. |
| `unzip` | Yes | `target:"artifacts"` | Must expose extracted local path and follow-up local search/structure continuations. |
| `clone` | Mostly | materialization lane | Search should use clone for proof; users may still need explicit clone/cache management until a materialization target lands. |
| `cache fetch` | Mostly | materialization/content handoff | Search should return materialized/localPath continuations; cache inspection remains management. |
| `diff` | Yes | `target:"diff"` | Direct file diff and PR patch diff need separate parity checks. |
| `search` | Already | OQL runner | Must stay a thin wrapper over tools-core. |
| `tools` | No | Raw tool runner | Keep for schema-exact debug, parity probes, and compatibility. |
| `context` | No | Protocol/help surface | Keep for agent bootstrapping and schema discovery. |
| `install`, `auth`, `login`, `logout`, `status` | No | Management | Keep outside OQL. |

## Agent Parity Procedure

Use this procedure before declaring `octocode search` a replacement for any
tool or command.

1. Read the live schema:

```bash
octocode search --scheme
```

Check that `activeTargets`, `query.target`, `from`, `params`, and target-specific
examples agree. Drift here is a blocker for agent trust.

2. Run a dry plan:

```bash
octocode search --explain --dry-run --query '<json>'
```

Check:

- `plan.normalized` has no CLI sugar.
- `plan.backendCalls[*].backend` is the same raw tool the old workflow used.
- `plan.backendCalls[*].exact` is honest.
- `plan.materialization` is explicit and bounded when GitHub source needs local
  proof.
- `plan.diagnostics` explains unsupported/residual/approximate behavior.

3. Run the OQL query with `--json`.

Check:

- `results[*].kind` is useful for the target.
- `diagnostics` do not block the answer.
- `provenance[*].backend` names the backing runner.
- `evidence.kind` is `proof` for answer-ready claims.
- `pagination.hasMore` and `next` are carried forward when more data exists.

4. Run the old command/tool for the same question.

Compare:

- row count and identity keys;
- paths, repo/ref, package, PR, commit, artifact identity;
- lines/ranges/snippets or payload bodies;
- pagination and continuation shape;
- diagnostics and failure mode;
- token density and human rendering.

5. Grade the replacement:

| Grade | Meaning |
|---|---|
| `replace` | OQL output is at least as complete, typed, and followable as old output. |
| `replace-json-only` | JSON is good, but human rendering hides important rows. |
| `adapter-present` | Backend call exists, but row/params/next parity is incomplete. |
| `not-yet` | OQL cannot express the workflow honestly. |

## Returned Data Requirements

All targets must return the common envelope:

- `results`
- `pagination`
- `next`
- `diagnostics`
- `provenance`
- `evidence`
- `plan` when `explain:true`

Target-specific minimums:

| Target | Minimum useful row data |
|---|---|
| `code` | source, path, line, column/endLine when known, snippet, metavars for structural search, row-level fetch/semantics continuation. |
| `content` | source, path, content, contentView, line/char range, char continuation. |
| `structure` | source, path, entryType, depth, size when available, drill/fetch continuation. |
| `files` | source, path, entryType, size/modified when requested, fetch/search continuation. |
| `semantics` | location/symbol/call/hover payload, uri, range, symbol identity, operation type. |
| `repositories` | owner, repo, stars/forks/language/topics/pushedAt, structure/search/clone continuations. |
| `packages` | name, version, description, downloads, repository, package subdir, repo/search/clone continuations. |
| `pullRequests` | number, title, state, author, dates, changed files, selected content, detail continuation. |
| `commits` | sha, title/message, author/date, touched path/ref, PR handoff when available. |
| `artifacts` | mode, format, entries/strings/symbols, extracted/decompressed localPath when created. |
| `diff` | file path, hunks/patches, additions/deletions, PR/file provenance. |

Generic `kind:"record"` rows are acceptable only as a transitional layer. Before
full replacement, agents need either typed row sub-shapes or documented
`recordType` payload contracts.

## Research Quality Gates

An answer is research-quality only when:

- `evidence.answerReady === true`;
- `evidence.kind === "proof"`;
- `evidence.complete === true`;
- every required predicate is represented in `plan.nodes` or target params;
- no blocking diagnostic is present;
- provider approximations are either acceptable to the task or followed by
  materialized proof;
- the agent can cite a stable identity: file:line, repo/ref/path, package
  name/version, PR/commit id, artifact path, or LSP location.

Do not treat these as proof:

- provider zero matches without complete predicate evaluation;
- `candidate` evidence;
- `partial` evidence caused by pagination/truncation;
- generic package/repo/history rows without checking the requested params;
- snippets when exact file content is required;
- stale clone/cache diagnostics without deciding whether freshness matters.

## Advanced Edge Cases To Check

| Area | Edge case | Required behavior |
|---|---|---|
| Schema drift | `activeTargets` differs from `query.target` help or diagnostics repair text | Fix source of truth before relying on agent instructions. |
| Opaque V2 params | `params` accepts anything but docs do not name fields | Read raw tool schema; add typed target docs before replacement. |
| Human rendering | `kind:"record"` rows are invisible or too terse | Use `--json`; renderer must support record rows before human parity. |
| V2 continuations | backing tool returns `data.next` instead of OQL `next` | Promote to OQL continuations so agents can follow them uniformly. |
| Pagination | result pages, per-file match pages, char offsets, archive entries, semantic rows | Preserve the exact pagination domain and expose executable continuation. |
| Batch merge | incompatible row kinds or pagination domains | Reject with repair diagnostic; do not silently merge. |
| GitHub regex | provider search cannot prove local regex semantics | Mark candidate/approximate or materialize bounded scope. |
| Structural search | GitHub cannot run AST search directly | Require bounded materialization. |
| Negation | provider source lacks complete universe | Require materialization or return `negativeUniverseRequired`. |
| Remote LSP | repo/file must materialize before LSP | Show clone provenance, local URI, and LSP diagnostics. |
| Binary/archive | extraction/decompression writes derived files | Return localPath and follow-up local `search`/`structure` continuations. |
| PR/history | broad list mode vs detail mode have different payloads | Preserve mode and content selectors; avoid pretending list rows include full detail. |
| Diff | direct file diff and PR patch diff are different workflows | Represent both explicitly or mark unsupported. |
| Auth/rate | GitHub/npm calls can fail externally | Return `rateLimited`, auth/error diagnostics, and non-proof evidence. |
| Secrets | output may be sanitized | Preserve `sanitized` diagnostic so agents know content changed. |
| Build freshness | built CLI may be older than source | Rebuild before scoring search behavior. |

## Current Gap Log

These are the high-value checks to close before `search` can replace all
research commands confidently:

1. `octocode search --scheme` must describe every active target in
   `query.target`, not only `code | content | structure | files`.
2. `unsupportedTarget` repair text must name the current active targets, not the
   old V1-only set.
3. V2 target `params` need schema text or per-target docs, otherwise agents
   still need raw `tools <name> --scheme`.
4. V2 adapters return `kind:"record"` rows. Either document exact `recordType`
   payloads or promote them to typed rows.
5. Human rendering must handle `kind:"record"`; until then, V2 targets are
   `--json` only for agents.
6. Backing tool `data.next` hints must become OQL `next` continuations.
7. `clone` and `cache fetch` need a first-class materialization/checkpoint
   story if search is meant to replace their user-visible workflows.
8. `diff` must distinguish direct local/GitHub file diff from PR patch research.
9. `unzip` replacement must prove archive unpack output and local follow-up
   paths, not just inspect metadata.
10. Tests should compare OQL JSON against raw tool JSON for each target, not
    only assert that planner routes to the right backend.

## Minimal Parity Test Suite

For each row in the raw tool matrix, keep one golden test:

1. Raw tool input.
2. Equivalent OQL input.
3. `--explain --dry-run` backend call assertion.
4. OQL execution JSON shape assertion.
5. Old-vs-OQL row identity comparison.
6. Diagnostics/evidence assertion.
7. Continuation assertion.
8. Human renderer assertion when the command is user-facing.

Recommended probe order:

```bash
octocode search --scheme
octocode search --explain --dry-run --query '<oql>'
octocode search --json --query '<oql>'
octocode tools <rawTool> --scheme
octocode tools <rawTool> --json --queries '<raw>'
```

## Replacement Readiness Summary

Current direction is correct: `search` delegates to tools-core, V2 adapters call
existing tool runners, and CLI shorthand lowering has moved into tools-core.

The remaining risk is agent trust, not basic dispatch. To fully replace all
research tools, make target-specific params, record payloads, continuations,
renderer output, and old-vs-new parity tests as strong as the existing V1
code/content/structure/files path.
