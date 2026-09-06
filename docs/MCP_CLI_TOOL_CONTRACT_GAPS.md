# MCP and CLI tool contract audit

This audit covers all 10 public Octocode tools as exposed by the built CLI and
the MCP stdio server on 2026-09-01. All tools executed successfully. Executable
input contracts now have one owner, and a committed offline matrix replays every
tool—including a second continuation page—through tools-core, CLI, and MCP.

This is a historical snapshot of the 2026-09-01 audit, not production approval.
Later fixes and verification can supersede its scores and open items. Use
[`MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md`](https://github.com/bgauryy/octocode/blob/main/docs/MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md)
for tool-routing guidance and [`OCTOCODE_TOOLS.md`](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_TOOLS.md) for the
field-level reference.

## Audit result

- Catalog: 10 tools.
- CLI execution: 10 of 10 representative calls succeeded.
- MCP execution: 10 of 10 representative calls succeeded through stdio.
- Change-specific contract tests cover tools-core, MCP, and CLI.
- Input verdict: strict envelopes and retired-alias rejection are in place.
- Output verdict: CLI and MCP discovery publish no output schemas. Runtime
  responses retain plain TypeScript contracts and the shared result envelope.
- Suite score: **9.4/10** across description, input schema, implementation,
  output behavior, and hints/errors.
- Monorepo P0 gaps: none. The sibling HTTP host has the release-order blocker
  below and intentionally fails closed rather than serving legacy contracts.

### Score rubric

Each tool receives a score out of ten in five dimensions:

1. Description accuracy and routing value.
2. Input-schema clarity and enforceability.
3. Implementation and CLI/MCP executor alignment.
4. Runtime output stability and adapter parity.
5. Hints and error recovery quality.

The score measures the public contract, not the usefulness of the underlying
provider or the completeness of every operation variant.

### Component scorecard

| Component | Score | Evidence | Remaining deduction |
|---|---:|---|---|
| `@octocodeai/octocode-core` | **9.8/10** | Prompt/output-types-only surface; redundant 13-entry registry, executable schemas, CLI generator, generated resources, and duplicate skill bundle removed; lint, typecheck, build, and focused surface tests pass | The aligned package has not been published and consumed from npm yet |
| `octocode-mcp` | **9.8/10** | Exact ten-tool SDK catalog, no output schemas, shared titles/descriptions/input schemas, ten-tool offline execution/continuation parity, real stdio call, full suite and build pass | Live provider drift remains an authenticated smoke concern |
| `octocode` CLI | **9.8/10** | Exact ten-tool catalog, all ten representative calls pass, local command-spec ownership, ten-tool parity, full suite/build pass, and brief-schema output is 20.5% smaller | Operation-specific runtime shapes and provider drift remain inherent |
| HTTP MCP host | **9.6/10** | Local canonical five-remote-tool selection, fail-closed catalog guard, 412 tests, lint, and build pass | Requires the aligned tools-core npm release plus deployed authenticated list/call smoke |

## Complete tool scorecard

`D`, `S`, `I`, `O`, and `H/E` mean description, schema, implementation,
output, and hints/errors. Output scores describe observed runtime contracts;
output schemas are intentionally not published.

| Tool | D | S | I | O | H/E | Mean | Main deduction |
|---|---:|---:|---:|---:|---:|---:|---|
| `ghSearch` | 9 | 10 | 10 | 9 | 9 | 9.4 | Output fields vary by operation |
| `ghSearchHistory` | 9 | 10 | 10 | 9 | 9 | 9.4 | Three operation-specific candidate shapes |
| `ghGetHistoryItem` | 9 | 10 | 10 | 9 | 9 | 9.4 | Four operation-specific detail shapes |
| `ghGetFileContent` | 10 | 10 | 10 | 9 | 9 | 9.6 | Directory materialization remains specialized |
| `ghCloneRepo` | 9 | 9 | 9 | 9 | 9 | 9.0 | Materialization remains intentionally specialized |
| `localSearch` | 10 | 10 | 10 | 9 | 9 | 9.6 | Four operation-specific output shapes |
| `localAnalyzeGraph` | 10 | 9 | 10 | 9 | 9 | 9.4 | Graph edges remain candidate evidence |
| `localGetFileContent` | 10 | 10 | 10 | 9 | 9 | 9.6 | Local path identity remains specialized |
| `lspGetSemantics` | 10 | 10 | 10 | 9 | 9 | 9.6 | Workspace inference is cwd-sensitive |
| `npmSearch` | 9 | 10 | 10 | 9 | 9 | 9.4 | Registry/provider availability varies |

## Alignment gaps

### P0: Publish the canonical tools-core catalog before enabling the HTTP host

The migrated sibling HTTP host selects the five canonical remote tools directly
from `@octocodeai/octocode-tools-core`. The latest published package
(`18.1.2`) still exports the retired 15-tool catalog, so the host cannot start
against npm without reintroducing aliases or duplicated schemas.

Acceptance criteria:

- Publish the aligned 10-tool tools-core package.
- Update the host dependency and remove the temporary workspace-resolution
  boundary.
- Run a real authenticated HTTP MCP `tools/list` and one tool invocation.

### P1: Keep live provider drift checks separate from offline parity

The deterministic matrix proves adapter and continuation behavior without
network state. A separately gated authenticated smoke job is still appropriate
for GitHub/npm provider drift; it must not weaken or replace offline CI.

## Resolved in the 2026-09-01 cleanup

- CLI and MCP discovery no longer publish output schemas or compact output-field
  summaries.
- Tools-core now owns all executable input schemas, relations, validation, names,
  descriptions, titles, availability, and runtime attachments. An architecture
  test rejects imports from the external core's retired schema/MCP surfaces.
- `@octocodeai/octocode-core` is prompt-and-output-types only. Its 13-entry
  title/schema registry, CLI generator, generated resources, and duplicate skill
  bundle were removed.
- CLI command-help types and specs now live with the CLI runtime; the retired
  core `/cli` entry point and the stale `clone` help record are gone.
- The committed ten-tool fixture proves schema identity, success, row errors,
  whole-call errors, and executable page-two continuations through tools-core,
  CLI, and MCP.
- A held-out ten-case routing eval preserved 10/10 correctness and 10 calls while
  reducing schema bytes 16.08%, prompt bytes 79.02%, and total routing bytes
  25.05%; the unified GitHub/local searches remain accepted.
- The shared direct-tool specification owns all 10 human titles; MCP no longer
  keeps a second title registry.
- The same specification now owns all 10 descriptions; CLI and MCP consume it
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
- A real MCP SDK client now lists all 10 descriptors and proves
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
- Whole-response character pagination now returns an executable same-tool
  `responsePagination.next` query; a real second-page replay succeeds.
- Sparse clones are complete relative to the requested sparse scope and no
  longer emit a false `continuationMissing` diagnostic.
- Union-validation errors preserve `queries.N` paths and lead with operation or
  selector relations for npm, local search, and LSP inputs.
- CLI configuration titles and descriptions derive from the canonical tool
  definitions; the second 10-entry UI copy registry was removed.
- The duplicate LSP display-schema export was removed. Knip's duplicate-export
  check is clean; its unconfigured whole-repository scan still reports dynamic
  skill entrypoints and package barrels as candidates, not proven dead code.
- Focused red-to-green tests cover all three changes.

## Merge candidates

| Candidate | Scope | Decision | Reason |
|---|---|---|---|
| Direct-tool input, description, title, availability, and runtime metadata | internal | **Merge** | One specification can generate CLI discovery, MCP registration, and conformance tests |
| Named MCP tool exports and `ALL_TOOLS` lookup | internal | **Removed** | The named aliases had test-only consumers; registration now uses the catalog directly |
| GitHub and local content-reader selector fragments | internal | **Merged leaf builder** | A parameterized selector builder shares only identical range/match machinery while preserving source-specific tools |
| PR, issue, and commit discovery | public | **Merged search contract** | Strict plural operations share discovery while preserving operation-specific validation |
| PR, issue, commit, and comparison detail | public | **Merged get contract** | Strict singular operations share exact lookup while preserving operation-specific identities |
| GitHub history pagination and continuation builders | internal | **Reject generic merge** | PR, issue, and commit pagination axes are materially different |
| Live CLI/MCP audit harness and schema contract tests | test | **Merged** | One committed matrix proves catalog, execution, continuation, and adapter parity |
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
## Recommended implementation order

1. Publish the aligned tools-core release.
2. Switch the HTTP host from its local workspace boundary to that release.
3. Run authenticated deployed HTTP catalog and execution smoke checks.
4. Add isolated model-trajectory trials before claiming routing-accuracy gains.

Do not merge public tools until a workflow benchmark shows fewer routing errors
or fewer calls without reducing evidence quality. Internal schema and metadata
deduplication does not need that public-surface migration cost.

## Verification receipts

The audit used the built monorepo artifacts, not source-only mocks:

- `node packages/octocode/out/octocode.js tools --json` returned 10 canonical
  tools with no legacy public names.
- Requests for each removed tool return an unknown-tool error with exit code 3.
- The real MCP SDK catalog test lists the same 10 names and no output schemas.
- The offline parity matrix passes for all 10 tools in tools-core, CLI, and MCP.
- The held-out routing benchmark passes 12 tests and its ACCEPT gates.

Primary source paths:

- `packages/octocode-tools-core/src/tools/directToolCatalog/`
- `packages/octocode-tools-core/src/tools/toolConfig.ts`
- `packages/octocode-tools-core/src/toolContract/`
- `packages/octocode-mcp/src/tools/toolConfig.ts`
- `packages/octocode-mcp/tests/scheme/all-tools.schema-contract.test.ts`
- `packages/octocode-tools-core/tests/tools/schemaExecution.test.ts`
