# Built CLI and MCP checks

Build first with `node skills-dev/octocode-dev/scripts/dev.mjs build:dev`, then run:

```sh
node packages/octocode-benchmark/local/e2e/harness/run-all.mjs
node packages/octocode-benchmark/local/e2e/harness/run-all.mjs transport,grammar
```

Hermetic suites run in isolated homes and workspaces. External traffic is denied; GitHub, registries and classification use loopback fixtures, and tool-health uses a mocked LSP protocol server. Registry HTTPS runs through a host-restricted proxy with a temporary CA supplied only to test subprocesses. The tests require `openssl` to generate that CA; the system trust store is unchanged. Default correctness checks do not assert wall-clock performance. Timeout unit tests use mocked subprocesses and virtual time.
A failure, timeout, or suite reporting no checks makes the runner fail. Logs and
counts are written under `results/`; `results/summary.json` identifies the suites run.

| Suite | Evidence checked |
|---|---|
| `acceptance` | Typed call extraction, exact source coordinates and line gaps, schema-owned defaults |
| `transport` | CLI/MCP input rejection; every Unicode source line across opaque pages; fresh CLI process replay; cursor surface binding and mutation rejection |
| `grammar` | All 13 configured parser fixtures and extensions: declarations, call captures, UTF-16 columns, syntax-tree source extent, source outlines and file-type search |
| `classification-retry` | Shared requests, retry bounds, usage accounting, persistent versus memory caches, account isolation, and exact judgment identity across fresh CLI processes |
| `tool-health` | All 14 tools: published input/output validation, transport status, malformed admission without provider work, batch behavior, advanced operations and field effects, exact cursor walks, typed related calls and CLI/MCP parity. Clone follows its CLI-only contract. |

`node skills-dev/octocode-dev/scripts/dev.mjs test:tools` runs tool-health directly.
Its optional `--deep` explores additional related calls. The loopback provider
fixtures live in `harness/tool-health/`; no paid classification call is made by the
hermetic suites. Reports include the contract fingerprint and native artifact SHA-256, and fail if that artifact changes during acceptance.

Advanced cases cover local search/read/AST/structure variants and all 12 LSP operations; all eight registries; GitHub refs, filters, history, reviews and patches; clone isolation and local edits; classification questions, assets, sections, retry/cache isolation, source mutation and unfiltered recovery. Mocked LSP cases check protocol handling, cyclic caller/callee graphs and Rust feature/default/cfg option routing—not the semantic accuracy or startup speed of an installed language server. Classification URL refusals prove no network or inference work occurs; pinned GitHub image revisions verify byte identity, judgment reuse and immutable replay. Shared schema and continuation checks also cover failures on nested reads. The fixtures compare independently known source lines, rows and cells rather than serialized snapshots.

Real LSP and OS process-tree integration are opt-in:

```sh
OCTOCODE_LIVE=1 node packages/octocode-benchmark/local/e2e/harness/run-all.mjs lsp-importers,process-tree
OCTOCODE_LIVE=1 node skills-dev/octocode-dev/scripts/dev.mjs test:tools lspSearch --deep
```

The real tool-health LSP route requires a TypeScript language server and an installed Rust toolchain with `rust-analyzer`, `cargo`, and `rustc`. Missing providers produce an explicit skipped-tool reason; complete integration acceptance requires zero skipped tools. To additionally check real wall-clock latency budgets, explicitly add `--performance` to the live tool-health command. The ordinary mock run retains elapsed-time observations for diagnostics, never as a pass/fail oracle. Safety deadlines still bound hung test subprocesses; testing those deadlines advances a fake clock rather than waiting for expiry.

An MCP test-client timeout cancels the request and retires the owned session. Further requests fail instead of silently overlapping timed-out work.

For real-provider acceptance, configure GitHub and classification credentials and run:

```sh
OCTOCODE_LIVE=1 node packages/octocode-benchmark/local/e2e/harness/run-all.mjs remote
```

That exercises all 14 tools through the built CLI and all 13 eligible tools through a fresh MCP server: the current checkout, authenticated GitHub lookup/search/history/read/clone flows, npm resolution, and real classification of literal text, a public text URL and a public image. The public text flow follows a GitHub redirect, checks final provenance and exact captured lines, then replays the saved read. It makes a small number of paid classification calls. Missing credentials, providers or language servers fail acceptance; they are not successful skips. Competitor, soak and published-version experiments are separate programs; their historical records are not acceptance evidence for the current contract.

## Coverage ownership

The previous suites repeated native behavior with public paging knobs, prose matching,
name-based continuation inference and synthetic repairs. Their replacements preserve
outcomes at the layer that owns them:

| Removed checks | Current owner |
|---|---|
| Usage repair, field-placement and brief snapshots | Core schema rejection suites; native admission tests; `transport` and tool-health malformed inputs |
| Chain, lead replay and navigation snapshots | Tool-health's schema-declared typed calls and verbatim cursor execution; `transport` process replay |
| Local reader, large-file and ranking matrices | Native `source_view`, `local_fetch`, `local_search`, `structure_search`, classification selection and `runtime_read_span` tests; tool-health page reconstruction |
| Classifier selection-plan internals | Native selection recovery, source/account binding and classification integrations; `classification-retry` retains process/cache boundaries |
| Real-repository grammar/capability score snapshots | `grammar` source fixtures; native AST/LSP capability tests; `lsp-importers` real server execution |
| Large GitHub, package and patch-format snapshots | Native GitHub/artifact provider suites, history/compare integrations and tool-health loopback services; `remote` retains deployment smoke |

Fixtures check evidence, not language-model answer quality. Missing providers or
servers are not converted into successful checks.
