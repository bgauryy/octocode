# MCP and CLI tool contract audit

This audit covers all 12 public Octocode tools as exposed by the built CLI and
the MCP stdio server on 2026-09-01. All tools executed successfully. The main
remaining risk is repeatable execution parity: conditional input rules are now
machine-enforceable, while the all-tool provider execution audit still depends
on a live smoke harness rather than a fully offline fixture matrix.

Use this page as the current gap and consolidation backlog. Use
[`MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md`](https://github.com/bgauryy/octocode/blob/main/docs/MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md)
for tool-routing guidance and [`OCTOCODE_TOOLS.md`](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md) for the
field-level reference.

## Audit result

- Catalog: 12 tools; 10 enabled by default; `ghListReleases` and
  `ghSearchDiscussions` are opt-in.
- CLI execution: 12 of 12 representative calls succeeded.
- MCP execution: 12 of 12 representative calls succeeded through stdio.
- Change-specific contract tests cover tools-core, MCP, and CLI.
- Input verdict: strict envelopes and retired-alias rejection are in place.
- Output verdict: CLI and MCP discovery publish no output schemas. Runtime
  responses retain plain TypeScript contracts and the shared result envelope.
- Suite score: **9.2/10** across description, input schema, implementation,
  output behavior, and hints/errors.
- P0 gaps: none.

### Score rubric

Each tool receives a score out of ten in five dimensions:

1. Description accuracy and routing value.
2. Input-schema clarity and enforceability.
3. Implementation and CLI/MCP executor alignment.
4. Runtime output stability and adapter parity.
5. Hints and error recovery quality.

The score measures the public contract, not the usefulness of the underlying
provider or the completeness of every operation variant.

## Complete tool scorecard

`D`, `S`, `I`, `O`, and `H/E` mean description, schema, implementation,
output, and hints/errors. Output scores describe observed runtime contracts;
output schemas are intentionally not published.

| Tool | D | S | I | O | H/E | Mean | Main deduction |
|---|---:|---:|---:|---:|---:|---:|---|
| `ghSearch` | 9 | 10 | 9 | 8 | 9 | 9.0 | Output fields vary by operation |
| `ghSearchHistory` | 9 | 10 | 9 | 8 | 9 | 9.0 | Three operation-specific candidate shapes |
| `ghGetHistoryItem` | 9 | 10 | 9 | 8 | 9 | 9.0 | Four operation-specific detail shapes |
| `ghListReleases` | 9 | 9 | 9 | 8 | 8 | 8.6 | Opt-in provider surface |
| `ghSearchDiscussions` | 9 | 9 | 9 | 8 | 8 | 8.6 | Opt-in provider surface |
| `ghGetFileContent` | 10 | 10 | 9 | 8 | 9 | 9.2 | Directory materialization remains specialized |
| `ghCloneRepo` | 9 | 9 | 8 | 7 | 8 | 8.2 | Materialization output is intentionally specialized |
| `localSearch` | 10 | 10 | 9 | 8 | 8 | 9.0 | Four operation-specific output shapes |
| `localAnalyzeGraph` | 10 | 9 | 9 | 8 | 9 | 9.0 | Graph edges remain candidate evidence |
| `localGetFileContent` | 10 | 10 | 9 | 8 | 9 | 9.2 | Local path identity remains specialized |
| `lspGetSemantics` | 10 | 10 | 9 | 8 | 9 | 9.2 | Workspace inference is cwd-sensitive |
| `npmSearch` | 9 | 10 | 9 | 8 | 9 | 9.0 | Registry/provider availability varies |

## Alignment gaps

### P1: Executable input-schema ownership is split across repositories

`packages/octocode-tools-core/src/tools/directToolCatalog/toolSpecifications.ts`
owns the 13 names, titles, descriptions, and input-schema attachments, while
some executable input schemas still originate in `@octocodeai/octocode-core`.

This conflicts with the repository architecture rule that tools-core owns public
tool schemas and descriptions, while the external core supplies the shared
prompt and reusable output types. The split makes schema and executor changes
harder to review atomically.

Acceptance criteria:

- Move the remaining executable input schemas, relations, and variants into
  tools-core.
- Keep only reusable data types and shared prompt material in the external core.
- Keep the build-time assertion that every direct tool has one description,
  title, availability, and runtime attachment.

### P1: Live adapter parity is not a committed 12-tool test

The current tests prove input-schema visibility, strict inputs, retired-alias
rejection, and selected executions. They do not run a
representative success response for every tool through every adapter. The full
live MCP audit used for this page is a temporary harness, so CI cannot reproduce
it.

Acceptance criteria:

- Commit a provider-fixture matrix for all 12 tools.
- Run each fixture through tools-core, the CLI adapter, and the MCP adapter.
- Compare normalized `structuredContent` across adapters.
- Keep a separately gated live smoke job for provider drift.

### P1: Merged-tool workflow benefit needs a held-out benchmark

`ghSearch` and `localSearch` have executor-parity coverage, but parity does not
prove that consolidation reduces calls, prompt bytes, or routing errors.

Acceptance criteria:

- Add held-out GitHub and local research tasks with legacy and unified routes.
- Measure task correctness first, then calls and total input/output bytes.
- Keep a merge only when correctness is unchanged and at least one cost metric
  improves without a guardrail regression.

## Resolved in the 2026-09-01 cleanup

- CLI and MCP discovery no longer publish output schemas or compact output-field
  summaries.
- The shared direct-tool specification owns all 12 human titles; MCP no longer
  keeps a second title registry.
- The same specification now owns all 12 descriptions; CLI and MCP consume it
  directly instead of rereading external metadata.
- MCP uses one registration adapter for basic and remote security modes. The
  duplicate adapters, metadata skip policy, metadata gateway, and private-Zod
  schema bridge were removed.
- Exceptional and sanitization-failure results retain `results: []`, preserving
  the shared result envelope without publishing an output schema.
- `localSearch` reports a missing root as `fileAccessFailed` instead of an empty
  success.
- CLI context help documents `--minimal`, no longer claims `--full` embeds
  schemas, uses valid cheap-view fields, and routes lean schema discovery to the
  compact form.
- The lean catalog is bounded below 4 KB and minimal context below 750 bytes in
  contract tests.
- The `npmSearch` keyword-discovery example now sends
  `keywords: ["schema", "validation"]`.
- Generated Draft 2020-12 schemas now encode npm XOR, content selector modes,
  GitHub directory/file separation, PR/issue list-detail modes, PR patch modes,
  commit history/compare modes, and LSP operation requirements as item unions.
- Generated-schema tests round-trip the actual input view and compare invalid
  and valid controls against executable validation.
- A real MCP SDK client now lists all 12 feature-enabled descriptors and proves
  exact title, description, input-schema, order, and no-output-schema parity.
- MCP callbacks and cancellation/pagination forwarding now apply to both remote
  and basic/local registrations.
- Operation-only GitHub code/repository searches fail at schema validation.
- Rootless inferred graph reachability returns low-confidence empty/unclassified
  evidence instead of marking the entire graph unreachable.
- Issue-to-PR retry instructions emit runnable `ghSearchHistory` guidance and
  fetch the selected item through `ghGetHistoryItem`.
- Named MCP lookup aliases and public exports of the three retired GitHub tool
  modules were removed.
- Focused red-to-green tests cover all three changes.

## Merge candidates

| Candidate | Scope | Decision | Reason |
|---|---|---|---|
| Direct-tool input, description, title, availability, and runtime metadata | internal | **Merge** | One specification can generate CLI discovery, MCP registration, and conformance tests |
| Named MCP tool exports and `ALL_TOOLS` lookup | internal | **Removed** | The named aliases had test-only consumers; registration now uses the catalog directly |
| GitHub and local content-reader selector fragments | internal | **Merged leaf builder** | A parameterized selector builder shares only identical range/match machinery while preserving source-specific tools |
| PR, issue, and commit discovery | public | **Merged search contract** | Strict plural operations share discovery while preserving operation-specific validation |
| PR, issue, commit, and comparison detail | public | **Merged get contract** | Strict singular operations share exact lookup while preserving operation-specific identities |
| GitHub history pagination and continuation builders | internal | **Reject generic merge** | PR, issue, commit, release, and discussion pagination axes are materially different |
| Live CLI/MCP audit harness and schema contract tests | test | **Merge** | One committed matrix must prove catalog, execution, and adapter parity |
| This audit and the workflow guide | documentation | **Keep separate owners** | This page owns measured gaps; the workflow guide owns usage and evidence escalation |

## Public tools to keep separate

- Keep `ghSearchHistory` and `ghGetHistoryItem` separate. Candidate discovery
  and exact detail have different identities, costs, and output bounds.
- Keep `ghGetFileContent` and `localGetFileContent` separate. Authentication,
  caching, path identity, and evidence provenance differ.
- Keep `ghGetFileContent` and `ghCloneRepo` separate. One is a bounded read; the
  other materializes state for repeated local analysis.
- Keep `localAnalyzeGraph` and `lspGetSemantics` separate. File topology is
  syntactic candidate evidence; LSP resolves symbol identity.
- Keep `npmSearch` and `ghSearch` separate. Package identity and registry
  metadata are not repository-search semantics.
- Keep releases and discussions opt-in rather than folding them into a generic
  history tool. Their provider capabilities and pagination models differ.

## Recommended implementation order

1. Commit the offline 12-tool execution fixture matrix.
2. Add held-out workflow measurements for the two unified search tools.
3. Move the remaining executable input-schema ownership into tools-core.
4. Semantically lint generated command patterns.

Do not merge public tools until a workflow benchmark shows fewer routing errors
or fewer calls without reducing evidence quality. Internal schema and metadata
deduplication does not need that public-surface migration cost.

## Verification receipts

The audit used the built monorepo artifacts, not source-only mocks:

- `node packages/octocode/out/octocode.js tools --json` returned 12 canonical
  tools with no legacy public names.
- The audit generated compact and full input-schema views for all 12 tools.
- Representative CLI calls succeeded for all 12 tools, with feature flags set
  for releases and discussions.
- A live MCP stdio session listed and called all 12 tools successfully. It also
  exercised all four `localSearch` operations.
- Change-specific focused tests passed:
  - tools-core title/schema parity and command patterns: 19;
  - MCP registration, title, and pagination contracts: 62;
  - CLI raw tool command contract: 27.

Primary source paths:

- `packages/octocode-tools-core/src/tools/directToolCatalog/`
- `packages/octocode-tools-core/src/tools/toolConfig.ts`
- `packages/octocode-tools-core/src/toolContract/`
- `packages/octocode-mcp/src/tools/toolConfig.ts`
- `packages/octocode-mcp/tests/scheme/all-tools.schema-contract.test.ts`
- `packages/octocode-tools-core/tests/tools/schemaExecution.test.ts`
