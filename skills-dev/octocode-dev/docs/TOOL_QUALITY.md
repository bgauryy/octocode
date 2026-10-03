# Tool quality and agent workflow acceptance

This contributor reference defines how to evaluate the 16 tools in the Octocode catalog. `ghCloneRepo` and `astRewrite` are CLI-only; MCP exposes the other tools when their availability gates are satisfied. It separates implemented contracts from the tests that establish quality.

| Need | Owner |
|---|---|
| Concept | `<repo>/docs/OCTOCODE_PROTOCOL.md` |
| Routing decisions | `<repo>/docs/OCTOCODE_RESEARCH_MANIFEST.md` |
| Parameters and defaults | `<repo>/docs/OCTOCODE_TOOLS.md` and live schemas |
| Response fields, path reconstruction, pagination layers, handoffs | `<repo>/docs/TOOL_DATA_CONTRACT.md` |
| Base-call rules (normative prose) | `<repo>/docs/OCTOCODE_TOOLS.md#how-every-tool-call-works` |

- Do not use a historical score or backlog entry as release acceptance. Keep ratings dated and evidence-backed.
- A documentation correction does not establish a runtime improvement.

Every acceptance run covers the shared contract and the tool-specific behavior:

- a strict `{ queries: [...] }` envelope (`clasify` takes its own query shape);
- 1–5 same-tool query rows;
- required `reasoning` and `goal` on every row (a top-level brief is not inherited; `next.*` continuations carry the producing query's brief);
- zero-based result `index` alignment and isolated row errors;
- `scheme` `variants` and `rules`;
- collection, content, and whole-response continuations, and typed terminal limits.

## Inspect the surface being tested

After you build the CLI, run from the monorepo root:

```bash
node packages/octocode/out/octocode.js scheme --compact
node packages/octocode/out/octocode.js scheme localFetch --view query --compact
node packages/octocode/out/octocode.js scheme ghGetHistoryItem --view query
```

- The catalog has 16 tools, with 12 enabled by default in MCP: `clasify` needs a classification key, `ghCloneRepo` is CLI-only, and the beta tools `astTopology` and `astRewrite` need `OCTOCODE_BETA` (or `local.beta`).
- Enabled tools depend on local-tool, clone, storage, allowlist, beta, and credential-gated `clasify` settings.
- MCP omits `clasify` when no classification key resolves (`OCTOCODE_CLASSIFICATION_API`, `OCTOCODE_JEV_KEY`, or `.octocoderc` `classification.api`) or when `OCTOCODE_CLASSIFICATION_API` is present but blank.
- Record the effective configuration and unavailable capabilities with each run. Enabling a tool does not install its language server or grant provider access.
- Public schemas, descriptions, and shared instructions belong to `@octocodeai/octocode-core`. The native runtime (`<repo>/packages/octocode-native/ARCHITECTURE.md`) owns execution and response shaping.
- Test both CLI and MCP when you change registration, schema projection, output formatting, or continuation rendering.

## Shared acceptance requirements

### Schema and description accuracy

- Exercise each operation's required fields, defaults, valid selectors, and rejected cross-operation fields through the public validation path.
- Compare `scheme` variants and rules with the public input schema. Nested selectors may need the default public view; abbreviation must not imply unsupported behavior.
- Execute documented examples with observed paths and identities. A schema-valid example does not establish runtime correctness.
- When you replace a contract, remove renamed public aliases and duplicated interface guidance. Keep a compatibility path only when explicitly required.
- Execute invalid-input recovery through the actual adapter: the rejected call names a valid correction, and the corrected call passes validation. MCP validates registered schemas before it invokes the tool callback.

### Agent routing measurements

- Export JSON Schema for inputs so fields with defaults stay optional.
- Test the actual host envelope. Score tool selection, semantic arguments, schema validity, transport validity, and completed execution separately. A correct tool name with the wrong branch or search string is an incorrect request.
- Freeze cases, graders, schemas, instructions, executable validators, model digest, and runtime before inference. Keep historical runs when you repair a grader.
- Include negative grader tests: wrong refs, unrelated keywords, unnecessary calls, and prose that mentions a capability without satisfying the request.
- Report sensor failures separately from model failures.
- Compare native calling and JSON emulation with the same canonical information and budget. Report first attempts separately from bounded repair with actual errors.
- Use untouched cases for acceptance. Fixture checks and a small routing sample do not establish reliability across models and hosts.
- Check context after provider-specific tool rendering. Disable silent truncation where supported and keep server diagnostics; a returned token count may describe an already-truncated prompt.

### Evidence and output integrity

- Check row-local `meta.evidence` and `meta.diagnostics` after public response shaping. Default output is minimal; request `debug: true` to see `meta`. Do not document a separate invented evidence or warning envelope.
- Inspect the registered descriptor as well as TypeScript interfaces. MCP must not publish `outputSchema`. Validate real structured results against the internal core/native validator, not static output types or a discovery descriptor.
- Check `meta.evidence.kind` and `confidence`, then the operation's actual completeness fields. Do not require a universal `answerReady` or `complete` field; none exists.
- Verify `none` views against selected source after expected security redaction. Test transformed views separately; a short result is not proof of fidelity.
- Keep source and revision anchors. Search ranking, AST shape, graph edges, package metadata, and LSP results have different evidence boundaries.
- Verify CLI compact output with hoisted `shared` values and `base` paths. Compare reconstructed meaning, not repetition on every row.
- Check mixed success/error batches. A successful outer MCP envelope does not prove that every query or collection succeeded.

### Lossless reachable pagination

- A test that asserts `hasMore:true` is insufficient. Execute returned continuation queries through the named tool and prove that their union covers the fixture: termination, stable identities, no missing items, no repeated windows.
- Cover every independent partial surface: result lists, file/match pages, nested graph data, diagnostics, source lines, transformed characters, history collections, patch windows, and response-text windows.
- Continuations keep filters, selected operation, view, snapshot/ref, and unrelated pagination axes.
- Semantic chunking treats requested length as a window target. Validate actual offsets and content coverage. Page counters are accurate or marked as estimates. Test tiny windows, a boundary-crossing token, multibyte text, empty results, and a final short page.
- When a provider or public cap makes continuation impossible, require a typed terminal-limit diagnostic. A numeric page or cursor without an executable call does not satisfy the contract.
- Mutable provider search can reorder between requests; fixture completeness does not establish snapshot semantics for live indexed search.

### Efficiency with preserved behavior

- Measure provider request count, cache reuse, returned characters, latency, and fixture coverage before and after a change, with the same corpus, query, revision, and output contract. Inspect cold and warm paths when caching changes.
- Reject fewer requests that lose a requested collection, fewer tokens that lose matched evidence, and faster results that lose reachable pagination.
- Use `<repo>/packages/octocode-benchmark/README.md` for measured comparisons. Record commands and artifacts with the result.

## Per-tool verification matrix

| Tool | Routing and schema checks | Content, pagination, and failure checks |
|---|---|---|
| `localSearch` | Exercise lexical text and regex queries independently. | Verify match continuations, exclusions, and zero-result diagnostics. |
| `localFetch` | Exercise path-only, full, range, match, and each supported view. | Preserve matched anchors; reconstruct transformed windows; verify effective fallback mode, redaction, and source lines. |
| `clasify` | Exercise Noul, Choice, and Score over supplied values and delegated unread resources. | Verify body-free page answers, coverage, focus scopes, provider failures, and cache reuse on later exact reads. |
| `structureSearch` | Exercise `tree` and `files` with name and metadata filters. | Verify pagination, snapshots, depth bounds, and exclusions. |
| `astSearch` | Exercise `match`, `syntaxTree`, and `symbols`. | Validate pattern/rule exclusivity and language selection; traverse captures, nodes, and results; expose parser and scan limits. |
| `astTopology` | Exercise all seven graph analyses. | Traverse results and diagnostics; expose unresolved edges and coverage limits; corroborate deletion candidates. |
| `astRewrite` | Exercise preview, stale snapshots, hash guards, and apply on an isolated fixture. | Verify overlap guidance, transaction recovery, changed bytes, and unchanged files. |
| `lspSearch` | Exercise document, workspace, anchored, and hierarchy operations. | Distinguish unavailable server, unsupported capability, failed anchor, and valid empty result; verify server provenance and paginated snapshots. |
| `ghSearchRepo`, `ghSearchCode`, `ghStructure` | Exercise code, repository, and tree variants (one tool each); reject branch selection for indexed code search. | Preserve candidate matches, immutable tree identity, metadata pages, indexing uncertainty, and provider-limit diagnostics. |
| `ghGetFileContent` | Exercise exact, compact, full, and paginated file views. | Compare local/remote matching and windows; verify pinned refs, repeated-outline prevention, security redaction, and rejection of directory inputs. |
| `ghSearchHistory` | Exercise PR, issue, and commit discovery with operation-specific scope. | Traverse discovery pages; preserve filters and exact-detail hints; check supported minification modes and provider-incomplete results. |
| `ghGetHistoryItem` | Exercise PR/issue identities, exact commits, comparisons, and selected content. | Independently traverse files, comments, reviews, commits, bodies, and patches; preserve omitted-patch/error state and immutable refs. |
| `ghCloneRepo` | Exercise default ref, full SHA, sparse path, refresh, and availability gates. | Verify checkout identity, reusable cache, isolated snapshots, concurrent mutation handling, rollback, and cleanup; report crash-recovery coverage separately. |
| `artifactSearch` | Exercise exact names, scoped names, and keyword discovery; reject mixed or empty selectors. | Follow keyword continuations, preserve repository subdirectory, distinguish registry failures, and verify authenticated-registry behavior where available. |

## Minification and smart-window matrix

Test controls only where the public operation supports them. Do not add `minify` or `concise` to operations that reject them.

| Surface | Supported content controls | Required comparisons |
|---|---|---|
| Local file fetch | `none`, `standard`, `symbols`; source selectors and line or UTF-8 byte windows. | Exact selected source, matched-text preservation, whole-file outline, fallback metadata, and continuation union. |
| GitHub file fetch | `none`, `standard`, `symbols`; source selectors and line or UTF-8 byte windows. | Same fixture pinned to a commit, extraction parity, and repeated calls at distinct offsets. |
| GitHub code search | `concise`; no public `minify` field. | Snippet transformation keeps useful matched evidence and correct positions; exact fetch stays available. |
| PR/issue discovery | `concise`; no public `minify` field. | Verify metadata selection and list continuations; reject unsupported content controls. |
| PR detail | `none`, `standard`; selected bodies, patches, comments, reviews, and commits. | Keep exact requested text with `none`; exercise every content surface and independent continuation. |
| Issue detail | Body/comment selectors and character windows; no `minify` field. | Keep selected text and complete reachable comments/body windows. |
| Commit/compare | `includeDiff`, path selection, file and character windows; no `minify` field. | Keep diff lines and immutable identity; distinguish absent/omitted patches from empty changes. |

- Build the native minification matrix from the runtime configuration, including filename overrides. Check every configured extension with meaningful syntax; configuration presence is not a correctness test.
- Keep grammar fixtures, outline fixtures, graph resolution, and real LSP-server runs as separate coverage dimensions. Reference: `<repo>/packages/octocode-native/docs/engine/SUPPORTED_LANGUAGES_AND_FEATURES.md`.
- Fixtures: comments with delimiters, strings with comment-like text, TypeScript type declarations/imports, JSX/TSX, data and markup, indentation-sensitive files, empty input, malformed input, and inputs around the native size limit. Check output validity and preservation, not only smaller byte counts.
- The chunk-boundary security property runs in the normal Rust suite. Run the manual CommonJS benchmark explicitly and record its profile, fixtures, runtime, and parity checks; an ignored annotation is not a passing receipt.

## Record release evidence

- Follow `<repo>/AGENTS.md` and the package commands in [DEVELOPMENT.md](DEVELOPMENT.md#build-test-lint). Release gates are in [RELEASE.md](RELEASE.md).
- Rebuild changed engine and native runtime packages, then the CLI, before you exercise the real tool path. Run relevant unit and integration tests, lint, type checks, and the affected CLI/MCP acceptance calls.
- Record each check as passed, failed, skipped, or unavailable, with its command and scope.
- Separate fixture tests from live provider access, supported routes from installed language servers, and host-platform builds from native-target release coverage. An ignored stress test is not a pass; a platform-name consistency check is not a successful build on each target.
- Keep open defects in a dated acceptance artifact with reproducible evidence and the required regression. Remove fixed backlog entries when their tests and real tool checks pass.
- Do not keep historical scores as a second source of truth. Do not turn testing requirements into claims that all environments passed.
