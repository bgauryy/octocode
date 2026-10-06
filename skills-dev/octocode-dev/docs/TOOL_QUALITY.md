# Tool quality and agent workflow acceptance

How to evaluate the tools in the Octocode catalog.

| Need | Owner |
|---|---|
| Concept | `<repo>/docs/OCTOCODE_PROTOCOL.md` |
| Routing decisions | `<repo>/docs/OCTOCODE_RESEARCH_MANIFEST.md` |
| Parameters and defaults | `<repo>/docs/OCTOCODE_TOOLS.md` and live schemas |
| Response fields, path reconstruction, pagination layers, handoffs | `<repo>/docs/TOOL_DATA_CONTRACT.md` |
| Base-call rules (normative prose) | `<repo>/docs/OCTOCODE_TOOLS.md#how-every-tool-call-works` |

Every acceptance run covers the shared contract and the tool-specific behavior:

- a strict `{ queries: [...] }` envelope (for `clasify` too) with 1–5 same-tool rows; a flat row or bare array is rejected;
- optional `mainGoal` and `reasoning` on every row: a row without them validates, a blank one is dropped, a top-level brief is not inherited, and `next.*` pages and `hints.*` leads carry the producing query's brief only when it sent one;
- zero-based result `index` alignment and isolated row errors;
- `schema` `variants` and `rules`;
- collection, content, and whole-response continuations in `next` (pages only), optional `hints` leads and `hints.text` tips, and typed terminal limits.

## Inspect the surface being tested

After you build the CLI, run from the monorepo root:

```bash
node packages/octocode/out/octocode.js schema
node packages/octocode/out/octocode.js schema localFetch --view query
```

- The catalog has 16 tools, with 12 enabled by default in MCP: `clasify` needs a classification key, and `ghCloneRepo` and the beta tools `astTopology` and `astRewrite` are CLI-only (the beta tools also need `OCTOCODE_BETA` or `local.beta`).
- MCP omits `clasify` when no key resolves (`OCTOCODE_CLASSIFICATION_API` or `.octocoderc` `classification.api`) or when `OCTOCODE_CLASSIFICATION_API` is blank. Local-tool, clone, storage, and allowlist settings also gate tools.
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

- With `debug: true`, check row-local `meta.evidence` (`kind`, `confidence`) and `meta.diagnostics` after response shaping, then the operation's own completeness fields (no universal `answerReady` or `complete` exists).
- MCP must not publish `outputSchema`. Validate real structured results against the internal core/native validator, not static output types.
- Verify `none` views against source after redaction, and transformed views separately; short is not faithful.
- Keep source and revision anchors; search, AST, graph, package, and LSP evidence differ. Verify CLI compact output (`shared`, `root`) by reconstructed meaning.
- Check mixed success/error batches: a successful MCP envelope does not prove every query succeeded.

### Lossless reachable pagination

- `hasMore:true` is insufficient: execute returned continuations through the named tool and prove their union covers the fixture (termination, stable identities, no gaps, no repeats).
- Cover every independent partial surface (lists, file/match pages, graph data, diagnostics, source lines, transformed characters, history collections, patches, response text). Continuations keep filters, operation, view, snapshot/ref, and other pagination axes.
- Requested length is a window target: validate actual offsets and coverage; mark estimated page counters. Test tiny windows, boundary-crossing tokens, multibyte text, empty results, a short final page.
- A provider or public cap that blocks continuation needs a typed terminal-limit diagnostic; a bare page number or cursor fails the contract.

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
| `astTopology` | All seven graph analyses. | Results and diagnostics; unresolved edges and coverage limits; corroborated deletion candidates. |
| `astRewrite` | Preview, stale snapshots, hash guards, apply on an isolated fixture. | Overlap guidance, transaction recovery, changed bytes, unchanged files. |
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
| GitHub code search, PR/issue discovery | `concise`; no `minify`. | Snippets keep matched evidence and positions; list continuations; unsupported controls rejected. |
| PR detail | `none`, `standard`; bodies, patches, comments, reviews, commits. | Exact text with `none`; every content surface and continuation. |
| Issue detail | Body/comment selectors, character windows; no `minify`. | Selected text; all reachable comment/body windows. |
| Commit/compare | `sections:["patches"]`, `include` path selection, file and character windows; no `minify`. | Diff lines, immutable identity; absent/omitted patch vs empty change. |

- Build the native minification matrix from runtime configuration, including filename overrides. Grammar fixtures, outline fixtures, graph resolution, and real LSP-server runs are separate coverage dimensions (`<repo>/packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md`).
- Fixtures: delimiter comments, comment-like strings, TS types/imports, JSX/TSX, data, markup, indentation-sensitive, empty, malformed, near-size-limit; check validity and preservation, not byte counts.
- The chunk-boundary security property runs in the normal Rust suite. Run the manual CommonJS benchmark explicitly and record profile, fixtures, and parity; an ignored test is not a pass.

## Record release evidence

- Follow `<repo>/AGENTS.md` and [DEVELOPMENT.md](DEVELOPMENT.md#build-test-lint); release gates are in [RELEASE.md](RELEASE.md).
- Record check status per `../references/fix-and-verify.md`. A documentation fix is not a runtime improvement.
- Separate fixtures from live providers, supported routes from installed servers, and host builds from native-target coverage.
- Keep open defects in a dated acceptance artifact with reproducible evidence; remove an entry when its regression and real tool check pass.
- A historical score or backlog entry is not release acceptance; never claim all environments passed.
