# Tool quality and agent workflow acceptance

How to evaluate the tools in the Octocode catalog. Owners: concept `<repo>/docs/OCTOCODE_PROTOCOL.md`; routing `<repo>/skills/octocode-research/references/tool-flow.md`; parameters `<repo>/docs/OCTOCODE_TOOLS.md` and live schemas; response fields, paths, pagination, handoffs and base-call rules `<repo>/docs/TOOL_DATA_CONTRACT.md`.

Every acceptance run covers the shared contract and tool-specific behavior:

- `{queries:[...]}` with one to five independent same-tool rows, or only an issued `{cursor}`; CLI and MCP repair a shape with one lossless reading (a bare row) and reject ambiguous shapes and unknown fields;
- zero-based result index alignment and isolated row failures;
- authored operations, defaults and typed selectors from core;
- `next.*`/responsePagination continue calls, related fresh calls and prose-only hints;
- exact source evidence, complete reachable pagination and explicit terminal limits.

Prefer behavior tests that execute the public request and its emitted calls; remove snapshots, prose equality and test-local schema interpreters. Assert selected evidence, scope, provider effects, source identity, replay isolation and complete walks. Keep coverage floors; never weaken a claim to hide a failure.

Write each check as Given–When–Then: given a fixture, configuration and surface (CLI, MCP or both), when the public call runs, then the observable result (rows, `next`, `related`, hints, diagnostics, exit code) matches the contract. Record a failed check as a 5W2H defect: what, where (`path:line`), when (the reproducing call), who or what it affects, why, how to fix, how much; `unknown` for a slot nobody verified.

## Inspect the surface being tested

After building the CLI, from the repo root:

```bash
node packages/octocode/out/octocode.js schema
node packages/octocode/out/octocode.js schema localFetch --view query
```

- This shows the effective CLI and MCP catalog. A classification key enables `clasify` and inline `classification`; a CLI call without it answers `missingConfiguration`. MCP's other opt-ins: `OCTOCODE_BETA` (both graph tools), `OCTOCODE_ENABLE_CLONE`, `OCTOCODE_SECURITY_CHECKS`. Filters and trust rules: [configuration matrix](../../../docs/CONFIGURATION.md#tool-availability-cli-and-mcp). The CLI runs graph tools as generated tool commands.
- Discovery makes no provider request. Test missing prerequisites and provider failures at invocation, separately from MCP visibility. An enabled clone in memory mode stays visible and reports its persistent-storage requirement when called. CLI ignores MCP visibility settings but keeps path and content security.
- Record effective configuration and unavailable capabilities per run (enabling a tool installs no language server and grants no provider access). Test CLI and MCP when registration, schema projection, formatting or continuation rendering changes.

## Shared acceptance requirements

### Schema and description accuracy

- Through public validation, exercise each operation's required fields, defaults, selectors and rejected cross-operation fields; compare `schema` with the input schema.
- Execute documented examples with observed paths and identities (schema-valid is not runtime-correct).
- Through the real adapter (MCP validates before the callback), a rejected call names a valid correction and the corrected call passes.

### Agent routing measurements

- Test the actual host envelope with the exported input JSON Schema (defaults stay optional). Score tool selection, semantic arguments, schema validity, transport validity and completed execution separately; grade negatives (wrong refs, unnecessary calls, prose that names a capability without using it).
- Freeze cases, graders, schemas, instructions, model digest and runtime before inference; keep old runs when you repair a grader. Report sensor vs model failures and first attempts vs bounded repair apart; use untouched cases for acceptance.
- Check context after provider-specific rendering with silent truncation disabled; a returned token count may describe an already-truncated prompt.

### Evidence and output integrity

- Check provenance and the operation's own completeness facts after response shaping; there is no universal answerReady or complete flag.
- MCP must not publish `outputSchema`. Validate real structured results against the internal core/native validator, not static output types.
- Verify content views against source after redaction, and transformed views separately; short is not faithful.
- Keep source and revision anchors; every output row owns its evidence; verify workspace-relative paths against the configured workspace. Check mixed success/error batches: a successful MCP envelope does not prove every query succeeded.

### Lossless reachable pagination

- Paginate before reducing response size: never clip or summarize requested evidence to meet a byte, token or `responseChars` target. Keep an indivisible oversized value whole and disclose it. Explicit transformed views and security redaction define the requested representation.
- `hasMore:true` is insufficient: execute the returned continuations through the named tool and prove their union covers the fixture (termination, stable identities, no gaps, no repeats).
- Cover every independent partial surface (lists, file/match pages, graph data, diagnostics, source lines, transformed characters, history collections, patches, response text). Continuations keep filters, operation, view, snapshot/ref and other pagination axes.
- Requested length is a window target: validate actual offsets and coverage. Test tiny windows, boundary-crossing tokens, multibyte text, empty results, a short final page.
- A provider or public cap that blocks continuation needs a typed terminal-limit diagnostic; a bare page number or cursor fails the contract.
- A numeric cursor (`page`, `importerPage`, `scanOffset`, `materializeOffset`, ...) advances only through `response/paging.rs`: `next_cursor` (or `admitted`) checks the query schema `maximum`, and past it writes the terminal limit (`terminalLimit`, `partialReasons:["cursorMaximum"]`, a warning). `terminal_limit` and `mark_partial` are the one writer of those signals. The table test in `paging.rs` covers every bounded cursor field of every tool, and a source scan fails on a hand-advanced cursor.
- An invalid continuation fails debug builds and tests. In release the continuation stage drops the invalid lead and keeps the row, marking it `partialReasons:["leadInvalid"]` with a warning.

### Efficiency with preserved behavior

Measure requests, cache reuse, returned characters, latency and fixture coverage before and after on the same corpus, query, revision and output contract (cold and warm when caching changes; see `<repo>/packages/octocode-benchmark/README.md`). Reject savings that lose a collection, evidence or reachable pagination.

## Per-tool verification matrix

| Tool | Check |
|---|---|
| `localSearch` | Text and regex queries independently; match continuations, exclusions, zero-result diagnostics. |
| `localFetch` | Path-only, full, range, match and each view; matched anchors, transformed windows, fallback mode, redaction, source lines. |
| `clasify` | Noul, Choice, Score over values and unread resources; body-free answers, coverage, focus scopes, provider failures, cache reuse on exact reads. |
| `structureSearch` | `view:"paths"` and `"details"` with name and metadata filters; listing/tree pagination, snapshots, depth bounds, exclusions. |
| `astSearch` | `match`, `syntaxTree`, `symbols`; structural `rule` composition, parser selection, captures, nodes, parser and scan limits. |
| `lspSearch` | Document, workspace, anchored, hierarchy operations; distinguish no server, unsupported capability, failed anchor, valid empty; provenance; snapshots. |
| `graphBuild` | Scope, `refresh`, storage, CLI/MCP beta availability, one query per batch; freshness, durable extraction resume, source invalidation, bounded spill, failed-build preservation, corrupt checkpoint rejection. |
| `graphQuery` | Every operation, selector, witness/resolution option and availability gate; pinned continuation union per collection; integrity errors, stale snapshots, grammar coverage, evidence, unresolved sites. |
| `ghSearchRepo`, `ghSearchCode`, `ghStructure` | Code, repository and tree variants (one tool each); code search uses the default-branch index, and an explicit `ref` requires `repo` and re-reads its hits; immutable tree identity, metadata pages, indexing uncertainty, provider-limit diagnostics. |
| `ghGetFileContent` | `content`, `compact`, `symbols`, full and windowed; local/remote parity, pinned refs, repeated-outline prevention, redaction, directory-input rejection. |
| `ghSearchHistory` | PR, issue, commit discovery with operation scope; pages, filters, field selection, exact-detail hints, provider-incomplete results. |
| `ghGetHistoryItem` | PR/issue, commit, compare, selected content; each of files, comments, reviews, commits, bodies, patches traversed alone; omitted-patch/error state; immutable refs. |
| `ghCloneRepo` | Default ref, full SHA, sparse paths, history depth, availability gates; ref resolved to a commit every call, a clean checkout at that commit reused without a fetch, local edits refused and preserved, concurrent mutation, rollback, cleanup, crash recovery apart. |
| `artifactSearch` | Exact, scoped, keyword (reject mixed or empty selectors); keyword continuations, repo subdirectory, registry failures, authenticated registries. |
| `securityCheck` | Every `operation`; `url` forms (purl, registry page, GitHub repository, commit, advisory); `release` on distributions only. Verdict equals OSV's own answer; repository facts, cautions, commit refs and advisories equal GitHub's REST record; cvss `vector` never a number, `score` only as GitHub publishes it; anonymous and authenticated, a refused GitHub read disclosed (`sourceFailed`), never clean. Live: [tool checks](../../../packages/octocode-benchmark/CHECKS.md). |

## Minification and smart-window matrix

Test only controls the public operation supports.

- File fetch (local and GitHub): `content`, `compact`, `symbols`; selectors; line or UTF-8 byte windows. Compare exact source, matched text, outline, fallback metadata and the continuation union; GitHub adds pinned commit, parity and distinct offsets.
- GitHub code search and PR/issue discovery: `fields` only (keeps each row's id), no `minify`. Snippets keep matched evidence and positions; unsupported controls are rejected.
- PR, issue, commit and compare detail: no `minify` (GitHub's verbatim text). Compare exact text across every content surface, selector, character window and continuation; for commit/compare also `sections:["patches"]`, `include` path selection, diff lines, immutable identity and absent/omitted patch vs empty change.

- Build the native minification matrix from runtime configuration, including filename overrides. Grammar fixtures, outline fixtures, graph resolution and real LSP-server runs are separate coverage dimensions (`<repo>/packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md`).
- Fixtures: delimiter comments, comment-like strings, TS types/imports, JSX/TSX, data, markup, indentation-sensitive, empty, malformed, near-size-limit; check validity and preservation, not byte counts.
- The chunk-boundary security property runs in the normal Rust suite. Run the manual CommonJS benchmark explicitly and record profile, fixtures and parity; an ignored test is not a pass.

## Record evidence

- Follow `<repo>/AGENTS.md` and [DEVELOPMENT.md](DEVELOPMENT.md#build-test-lint). Record each check per `../references/fix-and-verify.md` (status, command, scope). A documentation fix is not a runtime improvement.
- Separate fixtures from live providers, supported routes from installed servers, and host builds from native-target coverage; each claim names the run that supports it. Never claim all environments passed.
- Keep open defects (5W2H records) in a dated acceptance artifact with reproducible evidence; remove an entry when its regression and real tool check pass. A historical score or backlog entry is not acceptance.
