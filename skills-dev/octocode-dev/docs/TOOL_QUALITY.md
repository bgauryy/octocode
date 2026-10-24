# Tool quality and agent workflow acceptance

How to evaluate the tools in the Octocode catalog.

| Need | Owner |
|---|---|
| Concept | `<repo>/docs/OCTOCODE_PROTOCOL.md` |
| Routing decisions | `<repo>/docs/OCTOCODE_WORKFLOWS.md` |
| Parameters and defaults | `<repo>/docs/OCTOCODE_TOOLS.md` and live schemas |
| Response fields, path reconstruction, pagination layers, handoffs | `<repo>/docs/TOOL_DATA_CONTRACT.md` |
| Base-call rules (normative prose) | `<repo>/docs/OCTOCODE_TOOLS.md#how-every-tool-call-works` |

Every acceptance run covers the shared contract and tool-specific behavior:

- `{queries:[...]}` with one to five independent same-tool rows, or only an issued `{cursor}`; CLI and MCP reject bare input and aliases;
- zero-based result index alignment and isolated row failures;
- authored operations, defaults and typed selectors from core;
- `next.*`/responsePagination continue calls, related fresh calls and prose-only hints;
- exact source evidence, complete reachable pagination and explicit terminal limits.

Prefer behavior tests that execute the public request and its emitted calls. Remove obsolete snapshots, prose equality and test-local schema interpreters. Assert selected evidence, scope, provider effects, source identity, replay isolation and complete walks. A responsibility index is not execution coverage. Keep coverage floors; do not hide failures by weakening the behavioral claim.

Write each behavior check as Given–When–Then: given a fixture, configuration, and surface (CLI, MCP, or both), when the public call runs, then the observable result (rows, `next`, `related`, hints, diagnostics, exit code) matches the contract. A failed check is a 5W2H defect record: what, where (`path:line`), when (the reproducing call), who or what it affects, why, how to fix, how much; `unknown` for a slot nobody verified.

## Inspect the surface being tested

After you build the CLI, run from the monorepo root:

```bash
node packages/octocode/out/octocode.js schema
node packages/octocode/out/octocode.js schema localFetch --view query
```

- The catalog has 14 tools, with 12 enabled by default in MCP. `ghCloneRepo` is CLI-only; beta (`OCTOCODE_BETA` or `local.beta`) adds `clasify` (which also needs a classification key). File graphs are the `octocode graph ingest|query` CLI, not a catalog tool.
- `clasify` is omitted when no key resolves (`OCTOCODE_CLASSIFICATION_API` or `.octocoderc` `classification.api`), when `OCTOCODE_CLASSIFICATION_API` is blank, or when beta is off. Local-tool, clone, storage, and allowlist settings also gate tools.
- Record effective configuration and unavailable capabilities per run; enabling a tool installs no language server and grants no provider access. Test CLI and MCP when registration, schema projection, formatting, or continuation rendering changes.

## Shared acceptance requirements

### Schema and description accuracy

- Through public validation, exercise each operation's required fields, defaults, selectors, and rejected cross-operation fields; compare `schema` with the input schema.
- Execute documented examples with observed paths and identities; schema-valid is not runtime-correct. A replaced contract drops renamed aliases unless compatibility is required.
- Through the actual adapter (MCP validates before the callback), a rejected call names a valid correction and the corrected call passes.

### Agent routing measurements

- Test the actual host envelope with exported input JSON Schema (defaults stay optional). Score tool selection, semantic arguments, schema validity, transport validity, and completed execution separately. A correct tool name with the wrong branch or search string is an incorrect request.
- Freeze cases, graders, schemas, instructions, model digest, and runtime before inference; keep old runs when you repair a grader. Grade negatives: wrong refs, unrelated keywords, unnecessary calls, prose that names a capability without using it.
- Report sensor vs model failures and first attempts vs bounded repair apart. Compare native calling and JSON emulation at equal information and budget.
- Use untouched cases for acceptance. Check context after provider-specific rendering with silent truncation disabled; a returned token count may describe an already-truncated prompt.

### Evidence and output integrity

- Check provenance and the operation’s own completeness facts after response shaping; there is no universal answerReady or complete flag.
- MCP must not publish `outputSchema`. Validate real structured results against the internal core/native validator, not static output types.
- Verify content views against source after redaction, and transformed views separately; short is not faithful.
- Keep source and revision anchors; search, AST, graph, package, and LSP evidence differ. Every output row owns its evidence; verify workspace-relative paths against the configured workspace.
- Check mixed success/error batches: a successful MCP envelope does not prove every query succeeded.

### Lossless reachable pagination

- `hasMore:true` is insufficient: execute returned continuations through the named tool and prove their union covers the fixture (termination, stable identities, no gaps, no repeats).
- Cover every independent partial surface (lists, file/match pages, graph data, diagnostics, source lines, transformed characters, history collections, patches, response text). Continuations keep filters, operation, view, snapshot/ref, and other pagination axes.
- Requested length is a window target: validate actual offsets and coverage; mark estimated page counters. Test tiny windows, boundary-crossing tokens, multibyte text, empty results, a short final page.
- A provider or public cap that blocks continuation needs a typed terminal-limit diagnostic; a bare page number or cursor fails the contract.
- A numeric cursor (`page`, `matchPage`, `importerPage`, `scanOffset`, `materializeOffset`, …) advances only through `response/paging.rs`: `next_cursor` (or `admitted`) checks the query schema `maximum`, and past it writes the terminal limit (`terminalLimit`, `partialReasons:["cursorMaximum"]`, a warning). `terminal_limit` and `mark_partial` there are the one writer of those signals. The table test in `paging.rs` covers every bounded cursor field of every tool, and a source scan fails on a cursor advanced by hand.
- An invalid continuation is a defect that fails debug builds and tests. In release, the continuation stage drops the invalid lead and keeps the row, marking it `partialReasons:["leadInvalid"]` with a warning, so a page is never lost to a bad lead.

### Efficiency with preserved behavior

- Measure requests, cache reuse, returned characters, latency, and fixture coverage before and after on the same corpus, query, revision, and output contract; include cold and warm paths when caching changes.
- Reject savings that lose a collection, evidence, or reachable pagination. Measure with `<repo>/packages/octocode-benchmark/README.md`.

## Per-tool verification matrix

| Tool | Routing and schema checks | Content, pagination, and failure checks |
|---|---|---|
| `localSearch` | Lexical text and regex queries independently. | Match continuations, exclusions, zero-result diagnostics. |
| `localFetch` | Path-only, full, range, match, each view. | Matched anchors; transformed windows; fallback mode, redaction, source lines. |
| `clasify` | Noul, Choice, Score over values and unread resources. | Body-free answers, coverage, focus scopes, provider failures, cache reuse on exact reads. |
| `structureSearch` | `tree` and `files` with name and metadata filters. | Pagination, snapshots, depth bounds, exclusions. |
| `astSearch` | `match`, `syntaxTree`, `symbols`. | Pattern/rule exclusivity, language selection; captures, nodes, results; parser and scan limits. |
| `lspSearch` | Document, workspace, anchored, hierarchy operations. | Distinguish no server, unsupported capability, failed anchor, valid empty; provenance; snapshots. |
| `ghSearchRepo`, `ghSearchCode`, `ghStructure` | Exercise code, repository, and tree variants (one tool each); reject branch selection for indexed code search. | Candidate matches, immutable tree identity, metadata pages, indexing uncertainty, provider-limit diagnostics. |
| `ghGetFileContent` | Exact, compact, full, paginated views. | Local/remote parity; pinned refs, repeated-outline prevention, redaction, directory-input rejection. |
| `ghSearchHistory` | PR, issue, commit discovery with operation scope. | Discovery pages; filters and exact-detail hints; minification modes; provider-incomplete results. |
| `ghGetHistoryItem` | PR/issue, commit, compare, selected content. | Each of files, comments, reviews, commits, bodies, patches traversed alone; omitted-patch/error state; immutable refs. |
| `ghCloneRepo` | Default ref, full SHA, sparse path, refresh, availability gates. | Checkout identity, cache reuse, isolated snapshots, concurrent mutation, rollback, cleanup; crash recovery apart. |
| `artifactSearch` | Exact, scoped, keyword; reject mixed or empty selectors. | Keyword continuations, repo subdirectory, registry failures, authenticated registries. |

## Minification and smart-window matrix

Test only controls the public operation supports.

| Surface | Supported content controls | Required comparisons |
|---|---|---|
| Local / GitHub file fetch | `none`, `standard`, `symbols`; selectors; line or UTF-8 byte windows. | Exact source, matched text, outline, fallback metadata, continuation union; GitHub: pinned commit, parity, distinct offsets. |
| GitHub code search, PR/issue discovery | `fields` (keeps each row's id); no `minify`. | Snippets keep matched evidence and positions; list continuations; unsupported controls rejected. |
| PR detail | Bodies, patches, comments, reviews, commits; no `minify` (text is GitHub's verbatim text). | Exact text; every content surface and continuation. |
| Issue detail | Body/comment selectors, character windows; no `minify`. | Selected text; all reachable comment/body windows. |
| Commit/compare | `sections:["patches"]`, `include` path selection, file and character windows; no `minify`. | Diff lines, immutable identity; absent/omitted patch vs empty change. |

- Build the native minification matrix from runtime configuration, including filename overrides. Grammar fixtures, outline fixtures, graph resolution, and real LSP-server runs are separate coverage dimensions (`<repo>/packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md`).
- Fixtures: delimiter comments, comment-like strings, TS types/imports, JSX/TSX, data, markup, indentation-sensitive, empty, malformed, near-size-limit; check validity and preservation, not byte counts.
- The chunk-boundary security property runs in the normal Rust suite. Run the manual CommonJS benchmark explicitly and record profile, fixtures, and parity; an ignored test is not a pass.

## Record release evidence

- Follow `<repo>/AGENTS.md` and [DEVELOPMENT.md](DEVELOPMENT.md#build-test-lint); release gates are in [RELEASE.md](RELEASE.md).
- Definition of Done for an acceptance run: each check is recorded per `../references/fix-and-verify.md` (status, command, scope). A documentation fix is not a runtime improvement.
- Claim–evidence: separate fixtures from live providers, supported routes from installed servers, and host builds from native-target coverage; each claim names the run that supports it.
- Keep open defects (5W2H records) in a dated acceptance artifact with reproducible evidence; remove an entry when its regression and real tool check pass.
- A historical score or backlog entry is not release acceptance; never claim all environments passed.
