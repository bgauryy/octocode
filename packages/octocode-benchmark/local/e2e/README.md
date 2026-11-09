# Built CLI and MCP checks

Build first with `node skills-dev/octocode-dev/scripts/dev.mjs build:dev`, then run:

```sh
node packages/octocode-benchmark/local/e2e/harness/run-all.mjs
node packages/octocode-benchmark/local/e2e/harness/run-all.mjs transport,grammar
```

Hermetic suites run in isolated homes and workspaces. External traffic is denied; GitHub, registries and classification use loopback fixtures, and tool-health uses a mocked LSP protocol server. Registry HTTPS runs through a host-restricted proxy with a temporary CA supplied only to test subprocesses. In tool-health each provider request carries its call's credential (a token for GitHub and classification, a user npmrc for the loopback npm registry, the proxy URL for the registry proxy), and each tool has its own registry world, so tools running side by side never count or steer each other's requests. The run fails `attribution.complete` when a provider request carries no call's credential, git's first smart-HTTP request (the 401 challenge) excepted. The tests require `openssl` to generate that CA; the system trust store is unchanged. Default correctness checks do not assert wall-clock performance. Timeout unit tests use mocked subprocesses and virtual time.
A failure, timeout, or suite reporting no checks makes the runner fail. Logs and
counts are written under `results/`; `results/summary.json` identifies the suites run.

| Suite | Evidence checked |
|---|---|
| `acceptance` | Typed call extraction, exact source coordinates and line gaps, schema-owned defaults |
| `transport` | CLI/MCP input rejection; every Unicode source line across opaque pages; fresh CLI process replay; cursor surface binding and mutation rejection |
| `grammar` | Runtime grammar inventory and every owned extension on CLI and MCP: declarations, call captures, UTF-16 positions, syntax-tree extent, all fetch views, exact source search, file-type filters, structure files/tree, and strict LSP definition/reference protocol fixtures. Complete tiny-page walks and transport parity. |
| `classification-retry` | Shared requests, retry bounds, usage accounting, persistent versus memory caches, account isolation, and exact judgment identity across fresh CLI processes |
| `provider-shapes` | Real recorded provider answers (below): every field a provider sends reaches the output or is declared `unread`, and the output meets the published contract |
| `tool-health` | Every generated contract tool on CLI and MCP: published input/output validation, transport status, malformed admission without provider work, batch behavior, advanced operations and field effects, exact cursor walks, typed related calls and transport parity. The isolated MCP configuration enables graph, clone and classification tools. |

`node skills-dev/octocode-dev/scripts/dev.mjs test:tools` runs every hermetic suite but `grammar`; `test:e2e` runs them all.
Its optional `--deep` explores additional related calls. The loopback provider
fixtures live in `harness/tool-health/`; no paid classification call is made by the
hermetic suites. Reports include the contract fingerprint and native artifact SHA-256, and fail if that artifact changes during acceptance.

Advanced cases cover local search/read/AST/structure variants and all 12 LSP operations; all eight registries; GitHub refs, filters, history, reviews and patches; clone isolation and local edits; classification questions, assets, sections, retry/cache isolation, source mutation and unfiltered recovery. Mocked LSP cases check protocol handling, cyclic caller/callee graphs and Rust feature/default/cfg option routing—not the semantic accuracy or startup speed of an installed language server. Classification URL refusals prove no network or inference work occurs; pinned GitHub image revisions verify byte identity, judgment reuse and immutable replay. Shared schema and continuation checks also cover failures on nested reads. The fixtures compare independently known source lines, rows and cells rather than serialized snapshots.

The search permutation matrix derives every combination of `view`, `regexEngine`,
`caseMode` and `wholeWord` from the generated schema, plus each `unique` value
where `view: "matchOnly"` admits it. Each combination walks tiny pages and compares
the complete result with fixture source. Graph operations run on both transports;
structure views check projected paths and file sizes. The local research workflow
discovers a file, searches it, then executes its emitted source read and caller
query unchanged, checking exact source and call sites.

GitHub workflows also follow repository discovery through pinned structure,
code search and emitted source reads; history searches through emitted item
reads; and sparse pinned clones through local search and exact source reads.
Each workflow runs on both transports against independently known fixture evidence.

`coverage.inputValues` inventories finite enum, constant and boolean values from
every query schema and reports requested, accepted and missing explicit values
separately for CLI and MCP. Accepted input coverage is not a semantic assertion
or exhaustive coverage of unrestricted strings, numbers, or all cross-field
products. Omitted defaults do not count as explicitly exercised values. Required
continuations run to completion, with response pages before tool pages; optional
related-call exploration remains bounded and reports the calls it did not visit.
Missing explicit top-level `operation` or `view` values fail acceptance for each
selected tool on each transport; new primary variants therefore require cases.

## Recorded provider answers

`test-utils/provider-answers/<provider>/<case>.json` holds one tool call's real answers, for public packages and repositories only: OSV, NuGet, npm, PyPI, crates.io, Maven, Go, Packagist, RubyGems, GitHub REST and GraphQL. A fixture written by a parser's author holds what the parser reads; these hold what the provider sends. The Rust tests (`RouteHttp::recorded`, `tests/support/recorded.rs`) and this proxy (`tool-health/recorded-answers.mjs`) read the same files.

| Command | What it does |
|---|---|
| `run-all.mjs provider-shapes` | Hermetic, in `test:tools`: replays each recording through the built CLI. A provider field that neither reaches the output nor sits in the case's `unread` list fails, by field path |
| `dev.mjs answers:record [id,...]` | Re-records live, through the same proxy, so it holds exactly what the tool asks. It fails rather than writes when an answer holds the token or a private repository, or when trimming loses a field path or changes what the tool asks next. Keeps `unread` |
| `dev.mjs answers:record --accept-unread id,...` | Declares a case's current gaps `unread`. Read that diff: it says the tool does not show those fields |

What's stored: method, URL, `accept` and body of the request; `status`, `headers` (`content-type`, `link`, `etag`, `cache-control`, `retry-after` only), `next`, `body` and the raw answer's `digest`. Never a request header or cookie. Personal addresses become `person@example.test`. Long lists keep the items that add a field path, and long text its start; what another request of the same call carries stays, ignoring case: a whole token of a URL path segment, query value (`+` read as a space) or body text, for a value of three or more characters any part of one (an id the tool then reads, a service URL it then calls), or a request's path tail a value holds (`lodash/lodash` inside a repository URL). A value that only echoes the answer's own request routes nothing. A list or map keeps, per request, the item holding the most of what it carries, then one holder of each other such value; a long text is cut just past the last such value its own request did not carry, never shorter than the plain cut. The record step replays the trimmed recording, refuses to store it if the tool asks anything else, and prints each case's stored bytes beside the old size. `tool-health/recorded-answers.test.mjs` and the Rust test `no_recording_holds_what_the_secret_detector_redacts` check every file. Add a call in `tool-health/recorded-plan.mjs`.

## Live suites

Real LSP and OS process-tree integration are opt-in:

```sh
OCTOCODE_LIVE=1 node packages/octocode-benchmark/local/e2e/harness/run-all.mjs lsp-importers,process-tree
OCTOCODE_LIVE=1 node packages/octocode-benchmark/local/e2e/harness/run-all.mjs grammar-live
OCTOCODE_LIVE=1 node skills-dev/octocode-dev/scripts/dev.mjs test:tools lspSearch --deep
```

The real tool-health LSP route requires a TypeScript language server and an installed Rust toolchain with `rust-analyzer`, `cargo`, and `rustc`. Missing providers produce an explicit skipped-tool reason; complete integration acceptance requires zero skipped tools. To additionally check real wall-clock latency budgets, explicitly add `--performance` to the live tool-health command. The ordinary mock run retains elapsed-time observations for diagnostics, never as a pass/fail oracle. Safety deadlines still bound hung test subprocesses; testing those deadlines advances a fake clock rather than waiting for expiry.

`grammar-live` probes server availability for every runtime grammar extension,
then checks exact definition and reference locations on both transports with
installed servers. Missing servers are recorded as unavailable, so a successful
exit does not claim complete language-server coverage. It creates isolated
fixture projects and broker state, disables automatic server installation, and
cleans up afterward. CUDA fixtures use host-only parsing without CUDA libraries;
this does not verify a complete CUDA toolchain. Parser/protocol coverage in
`grammar` remains separate from real semantic-server evidence.

`security-live` checks `securityCheck` against OSV, GitHub and the public registries twice:
anonymous (a fresh home, no token variable, `gh` hidden) and authenticated (a fresh home with
`GH_TOKEN`, `GITHUB_TOKEN` or `gh auth token` as `GH_TOKEN`). Each probe asks the provider
itself in the same run and compares the tool's row with that answer: OSV ids for every verdict,
GitHub's REST record for repository facts and cautions, commit signature, heads and pull
requests, repository and global advisories (publisher, credits, cvss `{vector, score}`, EPSS),
and OSV's record for the cvss vector. The probes reproduce the securityCheck RFC's live evidence
(E1–E21) and the B169 validation matrix. OSV-only provider probes run once, anonymously.
GitHub allows 60 anonymous requests an hour per IP, which the E probes spend: the anonymous pass
runs only them, reading provider truth with the caller's token when there is one, and the B169
matrix runs authenticated. A row that meets the limit must disclose it as the
[securityCheck reference](../../../../docs/OCTOCODE_TOOLS.md#securitycheck) states; a call that waits for the reset
instead fails at the call deadline and the rest of that pass is skipped. A provider read the
limit refused is a skip. Without a token the authenticated pass is a skip.
`results/security-live.json` records every call as run.

```sh
OCTOCODE_LIVE=1 node packages/octocode-benchmark/local/e2e/harness/run-all.mjs security-live
```

An MCP test-client timeout cancels the request and retires the owned session. Further requests fail instead of silently overlapping timed-out work.

For real-provider acceptance, configure GitHub and classification credentials and run:

```sh
OCTOCODE_LIVE=1 node packages/octocode-benchmark/local/e2e/harness/run-all.mjs remote
```

That targets every public tool through the built CLI and a fresh MCP server: the current checkout, graph build/query, authenticated GitHub lookup/search/history/read/clone flows, npm resolution, and real classification of literal text, a public text URL and a public image. The public text flow follows a GitHub redirect, checks final provenance and exact captured lines, then replays the saved read. It makes a small number of paid classification calls. Missing credentials, providers or language servers fail acceptance; they are not successful skips. Competitor and published-version experiments are separate programs; their historical records are not acceptance evidence for the current contract.

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
