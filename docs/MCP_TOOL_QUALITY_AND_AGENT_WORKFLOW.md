# Tool quality and agent workflow acceptance

This contributor reference defines how to evaluate the ten public research tools in the Octocode agentic toolkit.
It separates implemented contracts from the tests needed to establish quality.
For routing decisions, read the [research manifest](OCTOCODE_RESEARCH_MANIFEST.md).
For parameters and defaults, use the [tool reference](OCTOCODE_TOOLS.md) and live
schemas. Do not use a historical score or backlog entry as release acceptance.

For response fields, path reconstruction, pagination layers, and producer-to-consumer
handoffs, use the [tool data contract](TOOL_DATA_CONTRACT.md). Keep ratings dated
and evidence-backed; a documentation correction does not establish a runtime improvement.

Every acceptance run must cover the shared contract as well as tool-specific behavior: a strict `{ queries: [...] }` envelope; 1–5 same-tool query rows; optional per-query `goal` and `reasoning`; zero-based result `index` alignment; isolated row errors; compact `variants` and `relations`; collection/content/whole-response continuations; and typed terminal limits. The [tool reference's base-call section](OCTOCODE_TOOLS.md#how-every-tool-call-works) is the normative prose summary.

## Inspect the surface being tested

After building the CLI, run these commands from the monorepo root:

```bash
node packages/octocode/out/octocode.js tools --json
node packages/octocode/out/octocode.js tools localFetch --scheme --json --compact
node packages/octocode/out/octocode.js tools ghGetHistoryItem --scheme --json
```

The discovery catalog contains 10 tools, with 9 enabled by default. Enabled tools depend on local-tool,
clone, storage, and allowlist settings. Record the effective configuration and
unavailable capabilities with each acceptance run. Enabling a tool does not
install its external language server or grant provider access.

Public schemas, descriptions, and shared instructions belong to `@octocodeai/octocode-core`.
[Tools-core](https://github.com/bgauryy/octocode/blob/main/packages/octocode-tools-core/ARCHITECTURE.md) owns execution and response shaping. CLI and MCP expose
those contracts through their respective interfaces. Test both when changing
registration, schema projection, output formatting, or continuation rendering.

## Shared acceptance requirements

### Schema and description accuracy

- Exercise each operation's required fields, defaults, valid selectors, and
  rejected cross-operation fields through the public validation path.
- Compare compact `relations` and variants with the full schema. Nested selectors
  may need the full schema; abbreviation must not imply unsupported behavior.
- Execute documented examples after substituting observed paths and identities.
  A schema-valid example alone does not establish runtime correctness.
- Remove renamed public aliases and duplicated interface guidance when replacing
  a contract. Preserve a compatibility path only when explicitly required.
- Execute invalid-input recovery through the actual adapter: the rejected call
  must identify a valid correction, and the corrected call must pass validation.
  MCP validates registered schemas before invoking the tool callback.

### Agent routing measurements

Export JSON Schema for inputs so fields with defaults remain optional. Test the
actual host envelope and separate tool selection, semantic arguments, schema
validity, transport validity, and completed execution. A correct tool name with
the wrong branch or search string is an incorrect request.

Freeze cases, graders, schemas, instructions, executable validators, model digest,
and runtime before inference. Preserve historical runs when repairing a grader.
Include negative grader tests for wrong refs, unrelated keywords, unnecessary
calls, and prose that mentions a capability without satisfying the request.
Report sensor failures separately from model failures. Compare native calling and
JSON emulation with the same canonical information and budget, and report first
attempts separately from bounded repair using actual errors. Use untouched cases
for acceptance; fixture checks and a small routing sample do not establish
reliability across all models and hosts.
Check context after provider-specific tool rendering. Disable silent truncation
where supported and retain server diagnostics; a returned token count may describe
an already-truncated prompt.

### Evidence and output integrity

- Check row-local `meta.evidence` and `meta.diagnostics` after public response
  shaping. Do not document a separate invented evidence or warning envelope.
- Inspect the registered descriptor as well as TypeScript interfaces. MCP
  publishes no `outputSchema`; static output types are not runtime validation.
- Check `kind` and `confidence`, then the operation's actual completeness fields.
  Do not require nonexistent universal `answerReady` or `complete` metadata.
- Verify `none` views against selected source after expected security redaction.
  Test transformed views separately; a short result is not proof of fidelity.
- Preserve source and revision anchors. Search ranking, AST shape, graph edges,
  package metadata, and LSP results have different evidence boundaries.
- Verify CLI compact output with hoisted `shared` values and `base` paths. Compare
  reconstructed meaning, not whether every value repeats on every row.
- Check mixed success/error batches. A successful outer MCP envelope does not
  prove that every query or requested collection succeeded.

### Lossless reachable pagination

A test that asserts `hasMore:true` is insufficient. Execute returned continuation
queries through the named tool and prove that their union covers the fixture.
Check termination, stable identities, no missing items, and no repeated windows.

Cover every independent partial surface: result lists, file/match pages, nested
graph data, diagnostics, source lines, transformed characters, history collections,
patch windows, and response-text windows. Preserve filters, selected operation,
view, snapshot/ref, and unrelated pagination axes in continuation queries.

For semantic chunking, treat requested length as a window target. Validate actual
offsets and content coverage; page counters must be accurate or explicitly marked
as estimates. Include tiny windows, a boundary-crossing token, multibyte text,
empty results, and a final short page.

When a provider or public cap makes continuation impossible, require a typed
terminal-limit diagnostic. A numeric page or cursor without an executable call
does not satisfy the reachable-data contract. Mutable provider search can reorder
between requests; fixture completeness does not establish snapshot semantics for
live indexed search.

### Efficiency with preserved behavior

Measure provider request count, cache reuse, returned characters, latency, and
complete fixture coverage before and after a change. Keep the corpus, query,
revision, and output contract comparable. Inspect both cold and warm paths when
the change affects caching.

Do not accept fewer requests if a requested collection disappears, fewer tokens
if matched evidence is removed, or a faster result if pagination becomes
unreachable. Use the [benchmark guidance](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/README.md)
for measured comparisons; record commands and artifacts with the result.

## Per-tool verification matrix

| Tool | Routing and schema checks | Content, pagination, and failure checks |
|---|---|---|
| `localSearch` | Exercise lexical text and regex queries independently. | Verify match continuations, exclusions, and zero-result diagnostics. |
| `localFetch` | Exercise path-only, full, range, match, and each supported view. | Preserve matched anchors; reconstruct transformed windows; verify effective fallback mode, redaction, and source lines. |
| `astSearch` | Exercise `match`, `files`, filesystem/syntax `tree`, `symbols`, and all six `topology` analyses. | Validate pattern/rule exclusivity and language selection; traverse captures, nodes, results, and diagnostics; expose parser/scan limits and unresolved edges; corroborate deletion candidates. |
| `lspSearch` | Exercise document, workspace, anchored, and hierarchy operations. | Distinguish unavailable server, unsupported capability, failed anchor, and valid empty result; verify server provenance and paginated snapshots. |
| `ghSearch` | Exercise code, repository, and tree variants; reject branch selection for indexed code search. | Preserve candidate matches, selected operation, immutable tree identity, metadata pages, indexing uncertainty, and provider-limit diagnostics. |
| `ghGetFileContent` | Exercise exact, compact, full, and paginated file views. | Compare local/remote matching and windows; verify pinned refs, repeated-outline prevention, security redaction, and rejection of directory inputs. |
| `ghSearchHistory` | Exercise PR, issue, and commit discovery with operation-specific scope. | Traverse discovery pages; preserve filters and exact-detail hints; check supported minification modes and provider-incomplete results. |
| `ghGetHistoryItem` | Exercise PR/issue identities, exact commits, comparisons, and selected content. | Independently traverse files, comments, reviews, commits, bodies, and patches; preserve omitted-patch/error state and immutable refs. |
| `ghCloneRepo` | Exercise default ref, full SHA, sparse path, refresh, and availability gates. | Verify checkout identity, reusable cache, isolated snapshots, concurrent mutation handling, rollback, and cleanup; report crash-recovery coverage separately. |
| `artifactSearch` | Exercise exact names, scoped names, and keyword discovery; reject mixed or empty selectors. | Follow keyword continuations, preserve repository subdirectory, distinguish registry failures, and verify authenticated-registry behavior where available. |

## Minification and smart-window matrix

Test controls where the public operation supports them. Do not add `minify` or
`concise` to operations that reject them.

| Surface | Supported content controls | Required comparisons |
|---|---|---|
| Local file fetch | `none`, `standard`, `symbols`; source selectors and line or UTF-8 byte windows. | Exact selected source, matched-text preservation, whole-file outline, fallback metadata, and continuation union. |
| GitHub file fetch | `none`, `standard`, `symbols`; source selectors and line or UTF-8 byte windows. | Same fixture pinned to a commit, extraction parity, and repeated calls at distinct offsets. |
| GitHub code search | `concise`; no public `minify` field. | Snippet transformation retains useful matched evidence and correct positions; exact fetch remains available. |
| PR/issue discovery | `concise`; no public `minify` field. | Verify metadata selection and list continuations; reject unsupported content controls. |
| PR detail | `none`, `standard`; selected bodies, patches, comments, reviews, and commits. | Preserve exact requested text with `none`; exercise every content surface and independent continuation. |
| Issue detail | Body/comment selectors and character windows; no `minify` field. | Preserve selected text and complete reachable comments/body windows. |
| Commit/compare | `includeDiff`, path selection, file and character windows; no `minify` field. | Preserve diff lines and immutable identity; distinguish absent/omitted patches from empty changes. |

Build the native minification matrix from the runtime configuration, including
filename overrides. Check every configured extension with meaningful syntax;
configuration presence alone is not a correctness test. Keep grammar fixtures,
outline fixtures, graph resolution, and real LSP-server runs as separate coverage
dimensions. See the
[language and feature reference](https://github.com/bgauryy/octocode/blob/main/packages/octocode-engine/docs/SUPPORTED_LANGUAGES_AND_FEATURES.md).

Include comments containing delimiters, strings containing comment-like text,
TypeScript type declarations/imports, JSX/TSX, data and markup, indentation-sensitive
files, empty input, malformed input, and inputs around the native size limit.
Check output validity and preservation requirements, not only smaller byte counts.
The chunk-boundary security property runs in the normal Rust suite. Run the
manual CommonJS benchmark explicitly and record its profile, fixtures, runtime,
and parity checks; an ignored annotation alone is not a passing receipt.

## Record release evidence

Follow the [monorepo verification instructions](https://github.com/bgauryy/octocode/blob/main/AGENTS.md) and the affected
package's build/test commands. Rebuild changed engine and tools-core packages,
then rebuild the CLI before exercising the real tool path. Run relevant unit and
integration tests, lint, type checks, and the affected CLI/MCP acceptance calls.

Record each check as passed, failed, skipped, or unavailable, with its command and
scope. Separate fixture tests from live provider access, supported routes from
installed language servers, and host-platform builds from native-target release
coverage. An ignored stress test is not a pass; a platform-name consistency check
is not a successful build on each target.

Keep open defects in a dated acceptance artifact with reproducible evidence and
the required regression. Remove fixed backlog entries when their tests and real
tool checks pass. Do not retain historical scores as a second source of truth or
silently turn testing requirements into claims that all environments passed.
