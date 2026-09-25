# Clasify benchmark

Historical sections below include a retired `semanticRerank` comparison. The current public surface has exactly one semantic tool, `clasify`; do not use the historical name or arm protocol for new runs. This document is retained as evidence for the direct-read default and optional Scout gate.

Latest campaign: [full local rebuild and actual Jev-assisted MCP research](#full-local-rebuild-and-actual-jev-assisted-mcp-research). Two fresh Sol/medium workers both scored 5/5; the assisted worker used Scout and Judge, with 10.34% more captured payload and 44,156 additional provider tokens.

## Frozen protocol — 2026-09-24

- Exactly two fresh workers: `gpt-6-sol`, medium effort, one per arm. Each answers Q1, Q4, Q8, Q9, Q17 in that order. No cross-arm messages or answer sharing; no nested workers.
- **Without:** classification credential explicitly blank; `clasify` and `semanticRerank` blocked by the campaign wrapper and absent from live guidance.
- **With:** same CLI and ordinary tools, classification and reranking enabled. Apply the tool's decision gate; no forced calls. A zero-use result is valid and cannot demonstrate classifier effectiveness.
- Both use the rebuilt local CLI. The rebuilt MCP is checked for equivalent availability/instructions before the run; it is not a second research arm.
- Same pinned revisions, repository scope, question text, question order, 18-minute arm budget, 16 calls per question, six setup calls per arm, and 60-second command timeout. Each arm keeps context across its five questions; this is not fresh-per-question isolation.
- Use only instrumented `scheme`, `ghSearch`, `ghGetFileContent`, plus treatment `clasify`. No shell retrieval, web browser, clone, local repository cache, answer keys, prior benchmark answers, or external research tools. Setup schema/help/error calls count.
- Every file read (including delegated classification reads) is pinned by the wrapper. GitHub code search discovers candidates on the indexed default branch; answers must be verified at the pinned SHA. Search/ref mismatch remains a limitation.
- `OCTOCODE_STORAGE_MODE=memory`, fresh CLI process per call: no shared persistent Octocode response cache. GitHub/provider upstream caching and network contention remain uncontrolled.
- Parent independently verifies source evidence against a claim checklist fixed before spawn. Exactly two workers means no extra blind judge; this is a single-pass diagnostic, not a statistical winner or release gate.

## Measures and decision rule

Primary: fully correct, evidence-complete questions / 5. Partial quality: fraction of required claim groups correctly supported. Unsupported absence/exhaustiveness claims fail the corresponding group.

Secondary: instrumented Unicode transcript characters = commands + delivered tool output + final answers. Report setup separately and also in arm totals. The package's existing `instrument_command.py` records verbatim artifacts, hashes, failures, elapsed tool time, and process resource observations; final answers use `record_answer.py`. This does **not** measure full host tokens, system prompts, reasoning, or dollar cost. Do not convert characters to claimed token savings. Classification provider input/output tokens are counted once per actual response; unknown usage stays unknown. Report tool calls, classification/rerank calls, errors, evidence reads, and per-question elapsed observations separately.

Exploratory KEEP only if treatment quality does not regress, every fairness/evidence guard passes, and geometric-mean paired transcript-character ratio (with/without) is at most 0.85. Otherwise CONTINUE with the observed failure or missing evidence. Do not change prompts, cases, scoring, or gates during the run. Five pairs and one pass are insufficient for a general superiority claim; the remaining five cases are unrun reserves. These public questions may be familiar and are not private uncontaminated holdouts.

## Ten questions

Adapted from the package's [canonical 30-question pool](compare/github-questions/README.md), retaining IDs and wording. Resolve branch/default references to the pinned revisions below. Q24's unspecified signature change is a task ambiguity: an eventual answer must make compatibility assumptions explicit rather than invent one.

### Q1 — Next.js route-regex result — run

In `vercel/next.js` on `canary`, locate the exported `getRouteRegex()` function.
Name its file, the internal helper it calls to parameterize the route, and the
top-level fields returned by `getRouteRegex()`.

### Q4 — Axios redirect implementation across repositories — run

Across `axios/axios` and `follow-redirects/follow-redirects`, trace how Axios's
Node adapter delegates redirect-following HTTP(S) requests. Cite the Axios
dependency field, import and transport-selection branch, then name the upstream
request type and methods that issue a request and process a redirect response,
with their files.

### Q8 — VS Code keybinding dispatch — run

In `microsoft/vscode`, identify the concrete workbench keybinding service class
and file. Then identify the base class, file, and public method that receives a
keypress for dispatch.

### Q9 — Fastify lifecycle contract — run

In `fastify/fastify`, report the documented order from Incoming Request through
User Handler, including `onRequest`, `preParsing`, Parsing, `preValidation`,
Validation, and `preHandler`. Then identify the per-route context property and
runner function used by `lib/route.js` to invoke `onRequest` hooks.

### Q17 — Next.js fetch request memoization — run

In `vercel/next.js`, explain how the App Router implements per-render fetch **request
memoization** — i.e. how identical `fetch()` calls made during a single render are
deduplicated. Identify: the function that installs the wrapped fetch onto the global
`fetch`; the two layers it composes and the file each is defined in; the API used to scope
the memoization to one render; what the deduplication cache key is derived from; and **all**
conditions in the deduplication layer that bypass memoization and call the original fetch.

### Q18 — Vite dependency-section membership — reserve, not run

In `vitejs/vite`, inspect `packages/vite/package.json`. For `lightningcss`, `sass`,
and `fsevents`, report every occurrence across `dependencies`, `devDependencies`,
`peerDependencies`, `peerDependenciesMeta`, and `optionalDependencies`, including
exact ranges. For each package, classify the consumer-install implications and
distinguish development-only presence from peer or optional-dependency status.

### Q21 — LangChain createAgent flow and graph — reserve, not run

In `langchain-ai/langchainjs`, the `createAgent()` function is exported from
`libs/langchain/src/agents/index.ts`. Report how many public overloads it
declares and the single runtime class its implementation constructs and returns.
Then trace the flow from `createAgent` into that class and the underlying
LangGraph state graph: name the state factory it invokes (in `annotation.ts`)
and enumerate the graph the agent builds — its nodes (including the
model-request and tools nodes) and the conditional edge targets used to route
between them. Present the result as an explicit node→edge graph.

### Q24 — Axios buildFullPath blast radius — reserve, not run

In `axios/axios`, locate the exported `buildFullPath` helper and name its file
and current signature. Then enumerate every in-repo call site that consumes it
(under `lib/`), citing `file:line` for each. For each call site, state which
arguments it passes and whether a change to `buildFullPath`'s signature would be
a breaking change or backward-compatible at that site. Distinguish call sites
that would break from those that would not.

### Q27 — React Query public API surface — reserve, not run

In `TanStack/query`, inspect the `@tanstack/react-query` package and enumerate
**all** public exports (hooks, components, and classes) from its main entry
point. Name the entry file that defines these re-exports and cite it. Group the
exports by category (query hooks, mutation hooks, provider/context components,
and utility/core classes).

### Q29 — MCP HTTP transport authorization flow — reserve, not run

In `modelcontextprotocol/modelcontextprotocol`, explain how the Model Context
Protocol handles **authorization** for the streamable HTTP transport. Identify
the authorization framework it builds on (e.g. OAuth 2.x), the specification file
that defines the flow, and describe the steps: how the client discovers the
authorization server, how it obtains a token, and how that token is presented on
subsequent HTTP requests. Cite the spec file and quote the lines that define the
token-presentation header and the discovery mechanism.

## Pinned revisions

| Repository | Commit |
|---|---|
| `vercel/next.js` | `d35864d5c5d7bc99a035c835db87ad35ff52899a` |
| `axios/axios` | `961241f6c19798eff16b0869486c125430a17961` |
| `follow-redirects/follow-redirects` | `0c23a223067201c368035e82954c11eb2578a33b` |
| `microsoft/vscode` | `7d05ee0755a3f69d4c72f1799c5e7863cb48a2f7` |
| `fastify/fastify` | `6ed472b6fe023cd0e6283badda231b84e45582d0` |
| `vitejs/vite` | `39ddf7ccf7e7469ff6a3ba37bca38c32ea804d6e` |
| `langchain-ai/langchainjs` | `a9ada857f22c4b746801e7723ffaa4c4d59db771` |
| `TanStack/query` | `077230bf29dcc2d30aa426cff10d278667665e48` |
| `modelcontextprotocol/modelcontextprotocol` | `ab3a39c13bd23be691c2760e1c6c5c15a64582e1` |

## Reproduce and audit

Run artifacts and the frozen wrapper are under `.octocode/octocode-eval-benchmark/clasify-ab-2026-09-24/` at the repository root. `questions.json`, `pins.json`, `rubric.json`, and `frozen.json` define the harness. `preflight.json` records tool availability and hashes; `catalog-*.json`, `schema-*.json`, and `mcp-*.json` preserve the actual agent surfaces.

Each research call uses `python3 <run>/run-tool.py <with|without> <Q-id|SETUP> <tool> '<json>' --compact`. It delegates to the existing package instrumentation, rejects prohibited features, and pins file reads. Keep raw logs and artifacts; never replace failed attempts with only a successful rerun. Workers write one answer file per question, which the parent records and grades after both workers finish. Compare paired cases, not just pooled bytes.

## Results

Completed one paired pass with two `gpt-6-sol` workers at medium effort. Both answered all five questions correctly with pinned evidence: **5/5 complete answers, 20/20 supported claim groups per arm**. The treatment made **zero Clasify or semantic-reranking calls**. Its five skip decisions were justified by exact, bounded source evidence. This evaluates optional-tool routing on these questions; it does not measure Scout/Judge effectiveness.

| Question | Without: characters | With: characters | With / without | Complete answers |
|---|---:|---:|---:|---|
| Q1 | 6,067 | 7,729 | 1.274 | Both pass |
| Q4 | 28,934 | 39,705 | 1.372 | Both pass |
| Q8 | 10,957 | 12,531 | 1.144 | Both pass |
| Q9 | 8,589 | 10,041 | 1.169 | Both pass |
| Q17 | 20,031 | 19,168 | 0.957 | Both pass |
| Setup, reported separately | 26,824 | 42,232 | 1.574 | Catalog and schemas |
| Total including setup | 101,402 | 131,406 | 1.296 | 5/5 per arm |

The geometric mean of the five question ratios is **1.175**: 17.5% more instrumented characters in the treatment. Including setup, the pooled increase is 29.6%. These are transcript-character observations, **not host-token or cost measurements**. Commands include wrapper-injected pinned revisions. Excluded from this proxy: worker packets, host scaffolding, reasoning, and a baseline wrapper-path typo that failed before instrumentation.

Both arms made 14 research invocations; setup added three baseline calls and four treatment calls. There were 19 versus 23 file-result rows, including repeated windows and one baseline empty lookup. Summed tool wall time was 29.858 versus 29.242 seconds; first wrapper call to reported completion was 185.365 versus 176.602 seconds. Parallel network contention, unmeasured host execution, and one pass make these timings descriptive only. Classification-provider token usage was zero in both arms because neither feature was invoked.

**Verdict: CONTINUE.** Quality held, but the frozen 0.85 character-ratio target was not met. No classifier benefit or general arm superiority is established. Q17 treatment discovery also returned two rate-limited GitHub search rows inside an exit-0 batch, while pinned file reads succeeded. The rate-limit asymmetry compromises a clean efficiency comparison; retain both error rows and all delivered output. Exit 6 pagination in other searches is partial coverage, not a runtime failure.

Validation: local CLI and MCP builds passed; live CLI/MCP preflight confirmed classification disabled in the baseline and enabled in treatment. All frozen harness and binary hashes matched after the run. Twelve logs passed `sumlog.py --strict`; ten final answers were recorded exactly once. Parent grading used a pinned source reference prepared before reading worker answers. The final global documentation check passed, as did all 12 local links in the changed documents. An earlier global check encountered an unrelated missing package README; that blocker was gone on the final check.

## Improvements to test next

1. **Load the Clasify schema only after the decision gate is met.** The unused treatment `scheme clasify --compact` call delivered 12,477 output characters plus 91 command characters. Removing that call would remove 12,568 characters in this trace; this is a trace calculation, not a rerun. Keep the short routing rule visible in main CLI output, help, and MCP instructions. Do not remove guidance or force classification to justify its existence.
2. **Keep exact lookup cases as skip controls.** Q1, Q8, and Q9 were resolved directly; Q4 and Q17 needed source tracing but no unresolved semantic choice. Larger files and exhaustive questions alone did not justify calling Clasify.
3. **Add separately frozen positive cases before claiming Scout/Judge value.** Scout cases should contain ambiguous unread candidate bodies where screening changes which source to inspect. Judge cases should supply evidence but leave a consequential semantic choice unresolved. Include insufficient-evidence cases where gathering facts is the correct action. Freeze these cases and their claim rubrics before running; do not relabel this pilot or tune against its answers.
4. **Measure the actual next action.** For each classification, record the unresolved decision, resource/question IDs, chosen next read/test, provider usage, coverage, and source verification. Judge correctness and saved reading, not tool-call count. Do not automatically chain Scout and Judge or use either as proof of absence.
5. **Repeat with better isolation.** Use fresh workers per question and multiple counterbalanced passes, a blind claim grader, controlled request pacing or independently budgeted GitHub quotas, and full host token receipts when available. Keep the five public reserve questions unrun until the next protocol is frozen; they are not guaranteed positive classification cases.

Detailed local evidence is in the run directory: `REPORT.md`, `metrics.json`, `grades.json`, `audit.py`, arm answer files, strict-log receipts, and original tool artifacts. The directory is local and ignored by Git; this document preserves the portable protocol, pinned revisions, result table, and limitations.

## Follow-up: schema and response audit

The follow-up on 2026-09-24 changed discovery to recommend `--view query`, retained `--view full` for audits, shortened repeated Clasify/reranking field guidance, and fixed the CLI/core availability wiring so Clasify's delegated-read schema matches MCP. Shared instructions now defer unfamiliar schema loading until after tool choice and distinguish path discovery, candidate snippets and exact evidence reads. The original A/B results above remain unchanged.

| Controlled comparison | Before proxy tokens | After proxy tokens | Reduction |
|---|---:|---:|---:|
| Registered MCP Clasify input schema | 1,941 | 1,681 | 13.4% |
| CLI GitHub search full contract → code-only query schema | 3,156 | 943 | 70.1% |
| Two-page search snippets → same 15 paths | 2,339 | 457 | 80.5% |
| Wide match window → complete pinned function | 481 | 264 | 45.1% |

Counts use `o200k_base` as a proxy, not Sol host/billing tokens. All MCP discovery plus instructions fell only 1.9% overall; selecting an existing focused CLI view accounts for much of the larger CLI reduction. Path-only output intentionally omits snippets, and exact source ranges require prior evidence. These comparisons do not establish new autonomous-agent savings or accuracy.

All 10 registered MCP validation shapes remained equivalent. Core 265, CLI 157, and MCP 195 tests passed, along with native contract tests, live CLI/MCP schema parity, unchanged pinned source bytes/ranges, and live Noul/Choice/Score acceptance. Local native regeneration records dirty-core provenance; the release-only clean-core gate remains separate. Raw measurements and the detailed report are under `.octocode/octocode-prompt-optimizer/schema-response-2026-09-24/`. The following experiment checks optional semantic-tool routing and deferred schema loading through MCP.

## Follow-up: identical MCP capabilities, prompt-only restriction

Two new `gpt-6-sol` workers at medium effort received the **same ten MCP tools, including Clasify and semantic reranking**. Only their arm instructions differed: the baseline expressly prohibited Clasify, semanticRerank and tool-based Scout/Judge; treatment permitted them when useful. Credentials and capabilities were not disabled or filtered for either arm. Questions, pins, rubric and budgets matched the five-case protocol above. At this stage, five reserve questions remained unrun; the next campaign below runs them.

Both answered **5/5 questions with 20/20 supported claim groups**. Neither used Clasify or reranking; both loaded only `ghSearch` and `ghGetFileContent` schemas, once each. The unused Clasify-schema load observed in the earlier CLI treatment did not recur. Both made **17 MCP calls plus three discovery actions**.

| Question | Prohibited: measured characters | Optional: measured characters | Quality |
|---|---:|---:|---|
| Q1 | 15,028 | 21,625 | Both pass |
| Q4 | 56,244 | 57,010 | Both pass |
| Q8 | 24,531 | 13,994 | Both pass |
| Q9 | 9,048 | 7,990 | Both pass |
| Q17 | 15,427 | 26,593 | Both pass |
| Common catalog setup | 9,893 | 9,893 | Identical |
| Total | 130,171 | 137,105 | 5/5 each |

This sensor counts surfaced catalog/schema/tool JSON, MCP request arguments/name and final answers. Selected schemas are charged where first loaded; discovery across the whole arm was identical at 22,881 characters, including the common catalog. Total `o200k_base` proxy tokens were 33,535 versus 35,210. Optional treatment used **5.3% more measured characters**, with a geometric-mean question ratio of 1.0484. Packets, hidden reasoning, host scaffolding and automatically attached tool definitions are excluded. These are partial context proxies, not actual host/billing tokens; the different sensor prevents a before/after savings claim against the CLI pilot.

The host's already-attached MCP descriptions were stale after rebuilding. Both workers therefore used an instrumented official-SDK connection to the rebuilt stdio MCP, with `initialize`, `listTools` and `callTool`. Every registered tool remained callable. The bridge checked the same complete catalog hash on each invocation, exposed shared instructions/tool descriptions and selected schemas on demand, preserved raw responses, and displayed `structuredContent` once when available. This exercises real MCP through a recording adapter; it does not measure direct host-native tool invocation or eliminate automatic registration cost.

An initial adapter defect passed the wrong SDK result-schema argument. Both workers stopped before receiving source evidence. The failed attempt is retained separately; after repair and a successful pinned-source smoke test, harness revision 2 was frozen and equal budgets restarted. The same workers resumed, retaining failed-call/schema context, and both repeated discovery. They were fresh at initial spawn, not after repair. Failed-attempt overhead is excluded from the table.

Parent source review and receipt audits confirmed pinned reads, equal catalog hashes, unchanged revision-2 harness files and baseline prompt compliance. Baseline Q4 encountered a pagination snapshot restart; treatment Q17 encountered one rate-limited search row. Both recovered through exact source reads. Receipts correct the treatment summary's omitted late Q4 verification call. Treatment revisited Q4 after reaching Q17, so execution was not strictly sequential. Baseline Q9's lifecycle citation stops at line 29 rather than line 31, although the fetched evidence includes the complete sequence. These limitations remain visible; no blind grading or statistical superiority is claimed.

**Verdict: CONTINUE.** Optional treatment did not meet this run's frozen 10% context-reduction condition, and zero semantic calls cannot demonstrate Scout/Judge effectiveness. The observable schema-routing check passed: neither arm loaded an unused Clasify schema. Keep these as skip controls and freeze separate ambiguous-evidence cases before evaluating semantic assistance.

The largest remaining waste came from retrieval choices: baseline Q1 returned 322 source lines versus 57 in treatment; Q8 returned 448 versus 56. Treatment Q17 returned 344 lines but only 195 unique lines—149 repeated lines, or 43.3%. Prefer exact deciding ranges and reuse fetched evidence before rereading for citation precision, while retaining complete code coverage for exhaustive questions.

Local artifacts: `.octocode/octocode-eval-benchmark/clasify-mcp-prompt-ab-2026-09-24/`. `REPORT.md`, `metrics.json`, `grades.json`, `audit.py`, frozen packets/catalog/protocol, all 40 successful-pass receipts and `invalid-attempt-1/` preserve the evidence. No production code changed during this experiment.

## New questions through the attached MCP — Jev allowed versus prohibited

Using the updated `skills/octocode-eval-benchmark` protocol, the next campaign ran the five previously unrun public questions: **Q18, Q21, Q24, Q27 and Q29**. The exact attached `mcp__octocode_local__` catalog was frozen. Both arms retained all ten tools; one prompt prohibited Jev/Clasify/semanticRerank/Scout/Judge, while the other allowed useful semantic assistance. No CLI research or separately spawned SDK MCP server was used.

Direct child-agent invocation failed before source retrieval: both agents initially saw the tool catalog but received `MCP tool octocode-local/ghGetFileContent is not available to the model`, followed by missing callable tools. Parent-native calls worked, including a Jev preflight returning `jev-1.13.0`. That attempt is preserved as **INVALID infrastructure**, not a quality score.

A separately frozen run used two new, history-free Sol/medium agents and the parent's working native MCP connection. Workers authored each JSON request; the parent invoked the attached method unchanged and returned the exact saved native response without interpretation. This **native MCP relay** is not direct child-host execution. Both workers had equal relay access, catalog, source pins and budgets; scheduling, shared caches/quota and response-file exposure remain confounds. Shared filesystem access cannot be technically isolated in this host, so the run is exploratory, not a sealed acceptance test.

| Result | Jev prohibited | Jev allowed |
|---|---:|---:|
| Correct questions | 5/5 | 5/5 |
| Supported claim groups | 16/16 | 16/16 |
| Native MCP calls | 15 | 17 |
| Clasify/reranking calls | 0 | 0 |
| Captured result/request/answer characters | 289,916 | 308,367 |
| `o200k_base` proxy tokens | 73,060 | 79,698 |

The allowed arm captured 6.4% more characters. This sensor counts each raw MCP result once, including both text and structured channels, plus arguments and answers. It excludes catalog reads, hidden reasoning, relay scaffolding and rereads. Treatment reported truncated/repeated display of one broad Q29 response; both reread source for citation precision. **Captured payload is not actual model context or billing**, and these numbers are not comparable to earlier campaigns' totals.

Both arms resolved the 13 LangChain overloads and conditional graph, Axios's three four-argument callers, the unspecified signature-change assumption, React Query's nested wildcard exports (including runtime symbols from `types.ts`), and the pinned OAuth flow. All 32 native request/result receipts reconcile; pinned file queries, identical catalogs, frozen inputs and baseline prohibition passed audit. Baseline has a minor Q24 declaration-line citation offset; the correct signature is present in its fetched evidence. No native error or nested error row occurred in the scored run.

**Verdict: INCONCLUSIVE for Jev effectiveness.** Both workers skipped Jev because exact evidence settled the questions. This is useful optional-tool routing evidence, not a with-Jev execution advantage or disadvantage. Positive Scout/Judge cases still need ambiguous unread evidence or consequential interpretation; do not force calls into these completed factual tasks.

The live catalog revealed a separate optimization target: all ten exposed descriptions contain an identical **4,168-character instruction prefix**, totaling **37,512 repeated characters beyond one copy** in 69,431 characters of metadata. An earlier raw SDK catalog holds shared instructions separately. A fresh built-server wire check confirmed shared guidance in `initialize.instructions` (4,466 characters in the updated build) and **zero shared prefixes in all ten `tools/list` descriptions**. Bare CLI output and `scheme` carry identical canonical instructions; root `--help`, `-h`, and `help` each append them once. CLI guidance includes CLI-only `ghCloneRepo`, so its availability-scoped instructions intentionally differ from MCP. The observed repeated prefix belongs to the host-exposed metadata, not this server's wire descriptions; host attachment/serialization needs investigation before claiming a context or billing saving. Verification receipts: `.octocode/octocode-prompt-optimizer/instruction-placement-2026-09-24/checks.json` and `mcp-wire.json`. Likewise, preserve both raw MCP response channels for debugging while displaying the appropriate channel once; archive size alone does not establish double model exposure.

Run evidence lives under `.octocode/benchmarks/jev-native-mcp/results/20260924T142643Z-relay/`: `summary.md`, frozen protocol/catalog/packets, `grades.json`, sealed answers, native traces, evaluator references and reproducible `audit.py`. The invalid direct-child attempt is preserved separately at `20260924T142305Z`. All ten questions listed earlier have now been exercised across the distinct campaigns; their original run/reserve labels describe the first pilot only.

## Full local rebuild and actual Jev-assisted MCP research

Run: `.octocode/benchmarks/jev-rebuilt-mcp/results/20260924-full-local/`. A complete local `build:dev` passed before two fresh `gpt-6-sol` workers at medium effort started. Both used the official MCP SDK over stdio to the newly built local server, with all ten tools advertised and identical credentials. Every call started a fresh server with memory-only storage. The baseline prompt prohibited all Jev features; the treatment requested useful Scout and Judge coverage. No Octocode CLI research or parent tool relay was used. This tests an explicitly assisted workflow rather than spontaneous optional adoption.

The five fresh canonical questions were Q5 (Vue hydration), Q7 (Zustand integration), Q19 (Node process dispatch), Q28 (Hermes memory), and Q30 (Chromium IPC). Tasks, revisions, budgets, rubric, packets and instrumentation were frozen before dispatch; both final outputs were sealed before grading.

| Measure | Without Jev | Jev-assisted |
|---|---:|---:|
| Correct questions / supported claim groups | 5/5 · 20/20 | 5/5 · 20/20 |
| Research calls | 25 | 30 |
| Scout / Judge | 0 / 0 | 1 / 1 |
| Selected schema loads | 3 | 4 |
| Captured characters including catalog setup | 413,717 | 456,508 |
| Jev input / output tokens | 0 / 0 | 43,485 / 671 |

The treatment captured **10.34% more payload** overall; the paired geometric-mean question ratio was **1.060**, missing the frozen exploratory KEEP threshold of ≤0.85 with no quality regression. The `o200k_base` payload proxies were 97,284 versus 105,831 tokens, excluding worker prompts (1,360 versus 1,464 proxy tokens). These measure emitted JSON, requests and answers, not actual model context or billing: terminal truncation, rereads, hidden reasoning and conversation overhead are not captured. Raw response archive size is a separate sensor.

Both semantic calls occurred on Hermes. Scout screened four unread modules across eleven pages for persistence, active-list, compression and long-term-memory roles: **42,870 input / 619 output tokens**. Two resources had partial coverage; the worker preserved that limitation and verified targeted source. Judge selected token-triggered active-context compression rather than a fixed-count durable store: **615 / 52 tokens**. Its supplied evidence summary already stated the deciding facts, so the call provides weak evidence of added value beyond direct reasoning. Requiring coverage likely encouraged it; do not turn a functional-test requirement into a production call quota.

On Hermes alone, captured payload rose **3.52%** (206,233 versus 199,222 characters), with 14 versus 10 calls. Loading `clasify` added 13,887 pretty-printed schema characters. Both workers loaded each selected schema once. The treatment also recovered one nested `contextLines` validation error; there were no MCP envelope or harness failures. Every worker receipt used matching MCP/native hashes and pinned file revisions, and the baseline made zero semantic calls.

**Verdict: no observed efficiency benefit in this exploratory run.** Both arms answered the required facts correctly; five public cases and one worker per arm do not establish general Jev effectiveness. The shared filesystem could not be access-isolated, the parent evaluator was not blinded, and the SDK presentation is not this app's native tool-attachment projection. The run is not a statistical acceptance test.

The rebuild/check work also found and fixed a real test-isolation problem: host system-proxy discovery consumed **25.106 seconds** while constructing a client for a loopback transport fixture. Disabling proxies only for `cfg(test)` clients reduced construction to **1.945 milliseconds**; production proxy behavior and deadlines remain unchanged. All **68 native Jev tests** then passed, alongside **265 core, 195 MCP and 157 CLI tests**. Live rebuilt-MCP Scout, Judge, Noul, Choice, Score and semantic reranking passed with `jev-1.13.0`.

Keep the existing optional decision gate. Skip Judge when held evidence already settles the claim. Scout a bounded ambiguous region only when its verdict changes the next read; broad role classification can consume substantial provider tokens without reducing the full workflow. Use distinguishable Choice labels and the built-in `insufficient` label instead of adding another absence label such as `unclear`. Measure schemas, provider tokens, retries and verification reads together before accepting an optimization.

Full receipts and analysis: the run's `summary.md`, `protocol.json`, `frozen.json`, `sealed-outputs.json`, `grades.json`, `metrics.json`, `supplemental-audit.json`, `token-proxies.json`, worker folders, independent source receipts, and build/test logs.

## Exploratory document relevance probe

The intended exploratory use is to decide which unread documents contribute evidence to a search. Broad file-role classification and judging already settled evidence do not measure that use well. On 2026-09-24, a separate development probe used the rebuilt local MCP server through the official SDK to screen seven paths discovered by a literal `cache` search. Each resource delegated a `localFetch` match window with ten context lines; the agent did not read these excerpts before the first screen.

The research goal was cache freshness and isolation between GitHub credentials. Version 1 asked for “substantive evidence.” After source inspection found useful evidence in a middling result, version 2 explicitly counted one concrete fact that answers only part of the investigation. The follow-up did not name files or expected labels. It is feedback-driven prompt development, not an independent test or a fresh-agent A/B comparison.

| Document | Initial Noul | Partial-evidence wording | Source inspection |
|---|---:|---:|---|
| `ADDING_CONFIG.md` | 0.05 | 0.27 | Selected excerpt is a generic configuration-schema example. |
| `CONFIGURATION.md` | 0.92 | 0.98 | Freshness deadlines, ETags, and lifecycle behavior contribute evidence. |
| `MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md` | 0.65 | 0.56 | Mostly verification requirements; background rather than a direct cache-policy answer. |
| `OCTOCODE_MCP.md` | 0.65 | 0.94 | Expired-cache maintenance and live credential reads contribute evidence. |
| `OCTOCODE_TOOLS.md` | 0.87 | 0.96 | Partial screen; exact follow-up confirms refresh and checkout-validation rules. |
| `SECURITY.md` | 0.48 | 0.92 | Line 97 explicitly states partitioned GitHub endpoint, credential, session, and cache identities. |
| `TOOL_DATA_CONTRACT.md` | 0.41 | 0.53 | Cache-hit status does not prove freshness; a cached checkout's HEAD does not verify working-tree contents. |

The last row remains useful despite a middling score. A hard 0.8 exclusion would still lose evidence. Use scores to prioritize reads, retain unresolved candidates, and check missed constraints. Relevance differs from completeness: a limitation or counterexample can matter even if a file is not the main answer source.

Version 1 used **11,865 input / 140 output Jev tokens**; version 2 used **11,858 / 140**. Both returned seven verdicts. One selected tool-reference page exceeded the 12,000-character cap and correctly remained partial with no safe within-page continuation. Source ranges on that clipped page must not be interpreted as a claim that all those lines were evaluated. Direct follow-up reads supplied evidence; clipping is not an optimization.

**Verdict:** useful evidence for refining relevance wording and preserving uncertain leads; no measured end-to-end efficiency or quality gain. There was one adaptive prompt revision, no isolated solver pair, no complete document-recall ground truth, and no strong direct-search baseline run. The seven source hashes were checked unchanged across the refined call and verification. Existing schemas were reused; this probe does not measure schema acquisition. The broader corpus and unselected portions of files remain outside its coverage.

For the next agent comparison, use exploratory tasks with overlapping terminology and distributed evidence: retry safety, offline requirements, cache freshness, and migration constraints. Give both arms identical tools, sources, budgets, and tasks; allow optional Jev in one arm without requiring Scout/Judge quotas. Grade relevant-evidence recall and supported findings, especially omitted constraints, alongside host context, provider usage, calls, and latency. Include exact-anchor controls and a strong direct search/section-read baseline. Freeze unseen validation cases after development; do not reuse this probe as a holdout.

Receipts: `.octocode/benchmarks/exploratory-relevance/results/20260924/` contains the initial protocol, both requests/results, MCP receipts, pre-refinement observations, source verification, and source hashes. The production-facing recipe is in [Exploratory relevance](../../docs/OCTOCODE_CLASIFY.md#exploratory-relevance-which-documents-are-worth-reading).

## Bounded local and GitHub relevance routing

Two additional development probes on 2026-09-24 tested read routing over complete, preselected sections. Both used the built local MCP server through SDK stdio, not CLI research. The protocol was saved before screening; one Noul relevance question applied to each candidate, passed as an unread read-tool request. Candidate bodies were read only after routing decisions were saved. The direct baseline then fetched the exact same candidate sections for cost comparison and a manual false-negative audit.

| Probe | Direct request + response characters | Scout + selected-read request + response characters | Jev input / output tokens | Relevant sections retained |
|---|---:|---:|---:|---:|
| Local admission-capacity release | 15,611 | 9,360 (**−40.0%**) | 3,911 / 60 | 1/1 in the three-region corpus |
| GitHub queued-versus-running cancellation | 8,060 | 9,842 (**+22.1%**) | 2,554 / 80 | 3/3 in the four-section corpus |

Response-only characters were 14,896 → 7,526 locally and 7,124 → 7,265 on GitHub. Counts are captured UTF-16 string characters, not model tokens, billing, or actual host consumption. They include screening instructions and verification requests, but exclude shared discovery/schema setup and evaluator reads. Both routes used identical server/native hashes, and every assessed scope had complete coverage; there were no provider, nested-tool, or MCP-envelope errors in these calls.

**Local:** the inspected `providers/classification/gate.rs:188–318` region shows release of global and per-call in-flight counts plus notification of waiters, and `GatePermit::drop` performs that release. It scored 0.95. `providers/artifact/http.rs:177–208` checks cancellation/deadlines without an admission-release rule (0.08); `regex/isolated.rs:141–306` kills/reaps subprocesses and joins IO work without showing occupied request-slot release (0.21). All three bodies were independently inspected after selection; source hashes stayed unchanged. This establishes a smaller captured route for this candidate corpus, not that an adaptive agent would need all three reads—the earlier symbol outline could already suggest the permit implementation.

**GitHub:** pinned README sections for `sindresorhus/p-limit` (90–112), `sindresorhus/p-queue` (184–235), and `SGrondin/bottleneck` (623–641) scored 0.94, 0.92, and 0.83. They document, respectively, clearing pending promises without cancelling running ones, operation-owned handling of an abort signal once running, and waiting for EXECUTING jobs during stop. The selected `yocto-queue` API section (38–77, 0.26) describes value-queue operations rather than executing-task cancellation. All useful sections were retained, but most candidates were useful and short enough that Scout overhead outweighed the avoided read. This is a **direct-read case**, not evidence of a GitHub efficiency win. Immutable refs are saved in `github-request.json`.

Two decision controls used no Jev call: README headings already expose `p-queue` interval-cap controls for a rate-limit lookup, and the fetched cancellation statements settle a simple running-versus-pending claim without Judge. These are parent routing checks, not independent agent-behavior measurements.

The audit also exposed a real runtime mismatch: `semanticRerank` injected an implementation-only rubric into general relevance questions unless certain role words were present. A regression for “Could this file contribute a fact, constraint, or counterexample about retry safety?” failed before the fix. Native now adds that rubric only for explicit implementation/definition lookups; general relevance keeps the supplied question. All fifteen native rerank tests passed after the change. Public guidance, examples, schemas, and shipped skills were aligned with this boundary.

**Decision:** keep bounded Scout as an option when it avoids substantial undecided reading; prefer direct reads for already-small sections or when most candidates must be read. Retain uncertain/partial candidates and source identity. No quality improvement, whole-file recall, end-to-end agent gain, or total-token reduction is established by these two scoped probes. Their manual labels and selected cases are development evidence, not sealed acceptance data.

Receipts and verification logs: `.octocode/benchmarks/relevance-routing/results/20260924/` (`protocol.json`, requests, `routing-before-verification.json`, MCP receipts, `metrics.json`, `audit.json`, local hashes, and red/green regression logs).

Integration validation passed: core 265 tests, MCP 195, CLI 157, native rerank 15, native clippy, full local development build, skill review, and documentation verification. Fresh MCP Scout and exploratory rerank probes prioritized the useful prose fixture (0.94 and 0.96 respectively); this is functional smoke evidence, not a quality benchmark. The wire audit found shared instructions once at MCP initialization, none repeated in ten tool descriptions, and matching CLI main/catalog/help instructions. Captured instruction + description + Clasify-schema characters changed from 17,001 to 16,392 (−609); this is a catalog payload sensor, not model usage, and concurrent catalog changes may contribute. Native/core fingerprints match; local regeneration records dirty-core provenance, so the separate clean-core publish guard remains in force. An intermediate CLI test saw genuine contract drift while rebuilding; the final rebuilt suite passed after updating its obsolete Scout wording assertion.

## Core Clasify contract follow-up — 2026-09-24

This is contract validation, not another agent A/B or a claim of quality improvement.
The audit covered the core input/output schemas, availability projection,
Scout/Judge descriptions, shared MCP/CLI instructions, executable examples,
public exports, native contract generation, and tests.

Fixed an availability edge case: zero exposed read tools previously restored the
full Scout allowlist. Judge-only surfaces now accept held `{value}` context and
omit delegated read-query fields. Exhaustive checks over all 1,024 read-tool
subsets verified 10,240 delegated admission decisions and retained Judge for every
subset. The regression failed before the fix and passes afterward.

Corrected the README's goal-free question, arbitrary byte-prefix example,
truncation/continuation guidance, and stale response-field claims. Also removed
the schema's promise of page IDs: query/resource/question IDs correlate ordered
pages; pages do not expose a separate ID.

Validation artifacts are under
`.octocode/benchmarks/relevance-routing/results/20260924/core-followup-*`.
The live MCP provider smoke uses a tiny synthetic memory-cache fixture to check
Noul, Choice, and Score response contracts. It is intentionally an exact fixture
for transport validation, not an example of when research should invoke Judge.

Final follow-up validation: 266 core, 195 MCP, and 157 CLI tests passed; core
lint/type checking and regenerated native contract tests passed. Core, CLI, MCP,
native runtime/addon, and engine addon builds completed. A fresh MCP call after
rebuilding returned schema-valid Noul/Choice/Score results from `jev-1.13.0` with
complete fixture coverage. The wire audit confirmed shared instructions in MCP
initialization and all three CLI help forms, with zero tool-description copies.
The Judge-only input schema measured 5,794 characters versus 7,282 for the full
read-tool schema (20.4% smaller); this is a capability-specific character measure,
not model tokens or a measured research-quality improvement.
