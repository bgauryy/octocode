# Pi runtime audit

This September 13, 2026 audit covers the Octocode extension's observable model
context, Pi lifecycle, terminal UI, MCP requests, and schema boundaries. It is an
implementation review. It does not measure model quality or establish production reliability.
See [the architecture](../ARCHITECTURE.md), [tools](TOOLS.md), and [UI contract](UI.md)
for the standing contracts.

## What reaches the agent

The extension composes shared policy from `agent-contracts` with Pi host facts and
enabled capability catalogs. Plans, recovery, memory, and physiology arrive in an
attributed turn message. Its `display: false` flag hides it from ordinary transcript
display, not from the model. The review checks those messages and tool results;
it makes no claim about the model's private reasoning.

Pi retains control of model execution, steering, compaction, and cancellation.
Awareness supplies observations and advice. Prompts describe the available paths;
runtime checks own permissions, schema validation, and delivery boundaries.

## Repairs and reasons

| Boundary | Repair | Reason and evidence |
|---|---|---|
| Prompt preparation | Abort an uncommitted preparation at `agent_start`; isolate state by session identity and scope | Pi catches `before_agent_start` errors before its active abort controller exists. [Installed-Pi tests](../tests/pi-prompt-preflight.test.ts) prove zero provider calls after rejection and successful retry after repair. |
| Reflection delivery | Prepare advisories, then commit only after complete prompt assembly | Budget failure must not consume a warning that never reached the model. [Recovery tests](../tests/turn-recovery.test.ts) exercise failed assembly, retry, and session replacement. |
| Observation identity | Include the monotonic tool counter in outcome identity | Two same-millisecond tool outcomes must persist separately; replay must still deduplicate. [Observation tests](../tests/pi-awareness-observation.test.ts) use the real Awareness store. |
| Questions and pickers | Observe operation cancellation, close through Pi's completion callback, release listeners, and cancel durable questions | An aborted operation must not retain a pending question or accept a late answer. [Question cancellation](../tests/ask-user-cancellation.test.ts) and [picker tests](../tests/ui-overlays.test.ts) cover active and pre-aborted requests. |
| Session cleanup | Handle renderer teardown through the resource cleanup boundary | A UI exception must not skip unrelated cleanup or leave disposal stuck. [Runtime tests](../tests/session-runtime.test.ts) inject renderer failure. |
| Working indicator | Key initialization by runtime identity and rendered theme output | Pi creates fresh event contexts; object identity caused repeated spinner resets. [Initialization tests](../tests/extension-ui-initialization.test.ts) cover event, theme, and runtime changes. |
| RPC inspectors | Require TUI mode and notify on unsupported modes | RPC reports UI availability but cannot render custom terminal components. [Inspector tests](../tests/runtime-inspector.test.ts) reject the silent no-op path. |
| MCP sampling | Forward token ceiling and temperature; preserve complete text; map terminal outcomes | Approval text must match execution, and failed or limited output must not claim ordinary completion. [Client tests](../tests/mcp-client-handlers.test.ts) connect the MCP SDK to Pi's actual model registry and a deterministic provider. |
| MCP elicitation | Display the form schema or destination URL before consent; validate edited JSON | Approval needs the requested contract, and accepted values must satisfy it. The client tests exercise both direct handlers and a real MCP connection. |
| Schema dialects | Reject declared legacy dialects instead of applying modern semantics | Draft-03/04 required fields and bounds, and draft-06/07 `$ref` siblings, have incompatible semantics. [Validator tests](../tests/mcp-schema-validator.test.ts) cover explicit rejection and modern constraints. |
| Integration coverage | Run lifecycle scenarios with the production extension as well as an empty SDK composition | SDK-only success does not establish extension compatibility. [Production probes](../tests/pi-production-probe.test.ts) cover streaming, denial, failure, steering, sessions, compaction, UI semantics, and cancellation. |

Field validation, cancellation tests, and semantic UI fixtures have separate owners
after cleanup. File-size limits and test assertions remain intact.

## Awareness and worker follow-up

The follow-up review traces the host-neutral worker rules, Pi capability grants,
agent-visible instructions, native Awareness descriptors, lifecycle observations,
and parent inspection. Root and worker prompts are separate contexts: each retains
its authority rules. Shared wording alone is not evidence of duplicate delivery.

| Boundary | Repair | Evidence |
|---|---|---|
| Native schema discovery | Add `describe:true` without execution parameters; classify it as a read in the plan-mode hook | [Native contract tests](../tests/awareness-native-protocol.test.ts) inspect every canonical descriptor without storage; [worker tests](../tests/worker-production.test.ts) use installed Pi without shell access. |
| Canonical ownership | Delegate parameter validation, effect selection, and approval classification to Awareness descriptors; remove repeated Message instructions from Pi's tool guidelines | [Canonical tool tests](../tests/awareness-canonical-tool.test.ts) check the shared instructions and runtime validation. |
| Continuations | Translate nested retries, pages, and History undo previews; validate narrower retries before offering them | Native contract tests execute restore and undo preview, then recover five complete Message bodies across budget retries and pages. |
| Sensor meaning and binding | Do not report cancellation or permission blocks as failures; retain prior delivery identities across missing samples; bind queued writes before a session switch | [Observation tests](../tests/pi-awareness-observation.test.ts) reproduce each failure, including the real Awareness client and separate databases. |
| Worker handback state | Keep inspection read-only; retain reviewer verdicts and terminal summaries without a duplicate result line; map unrecognized confidence values to uncertain | [Handback tests](../tests/worker-handback.test.ts) retain transcripts while rejecting stale completion during follow-up and false confidence from values such as `unconfirmed`. |
| Duplicate implementation | Share forbidden worker names through agent-contracts and worker metadata through one Pi module; use the canonical observation type | [Capability tests](../../octocode-agent-contracts/tests/capabilities.test.ts), handback tests, and existing worker reliability checks cover the consumers. |
| Worker reasoning | Preserve explicit thinking levels and Pi defaults; remove the provider-name override | [Worker reliability tests](../tests/agent-reliability.test.ts) cover explicit and omitted levels; installed-Pi worker tests inspect the provider's received reasoning option. Pi's provider adapter owns transport compatibility. |
| Model context window | Bound prompt/tool overhead by the selected model's valid declared window as well as the harness ceiling | [Preflight tests](../tests/pi-prompt-preflight.test.ts) reject oversized assembly before a provider call, then recover after model selection changes. [Budget tests](../tests/context-segments.test.ts) cover boundaries and unavailable metadata. |

Prompt-optimizer review score for the native Awareness delivery surface: clarity
4→4, enforcement 3→4, structure 4→4, density 3→4, output 3→4, and integrity 4→4
(average 3.5→4.0, both B). These are rubric judgments, not model-performance
measurements. The concrete changes remove duplicate field guidance, provide an
executable discovery path, and distinguish partial recovery from a blocked call.

## Architecture cleanup

Callers import `QueryBatchError`, activity presentation helpers, and
`withPeerCoordination` from their owning modules. Removing three forwarding export
statements avoids extra dependency paths without changing the implementations.
Symbol-reference checks identified the consumers before the imports changed.

The source topology scan covered 229 files with no skipped files or unresolved
internal imports. Its three cycles each contain a type-only edge; it found no
runtime cycles. One dynamic CommonJS `require` in `ffmpeg-runtime.ts` remains
outside graph coverage. The cleanup retains the type-only cycles because the
review found no runtime defect that justified restructuring those boundaries.

The packaging test compares the executable Awareness CLI catalog against
`ROUTINE_AWARENESS_OPERATIONS`, including duplicate detection through exact array
comparison. Explicit membership checks also cover anchored memory and experience
history. The canonical catalog owns the inventory; Pi does not repeat its count.

## Upstream comparisons

[Pi's extension lifecycle](https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/coding-agent/docs/extensions.md)
and [runner implementation](https://github.com/earendil-works/pi/blob/71dca871bc80b6bc97be37f0ca3189399d651fff/packages/coding-agent/src/core/extensions/runner.ts)
are the API reference. Their live model getter also disproved a suspected stale
footer model: retaining that context does not freeze its model value.

[pi-mcp-adapter](https://github.com/nicobailon/pi-mcp-adapter/blob/main/README.md)
uses cached metadata, lazy connections, and optional direct tools. Those choices
offer a startup trade-off, not evidence that replacing Octocode's exact enabled
catalog is better. A lazy-start change needs cold-start and schema-freshness
measurements before adoption.

[agent-stuff](https://github.com/mitsuhiko/agent-stuff/blob/main/README.md)
demonstrates focused Pi extensions, prompt commands, and interactive tools.
This review uses those boundaries as design examples, without claiming a measured
ranking against either project.

The [official JSON Schema regression corpus](https://github.com/json-schema-org/JSON-Schema-Test-Suite/blob/f6fd52a0a95472e079cbfc6ef7f089702b80e045/tests/draft7/ref.json)
confirms the legacy `$ref` sibling difference. Supported declarations and migration
requirements belong in the [MCP tool contract](TOOLS.md#4-use-mcp-tools).

## Verification limits

### Recorded checks

The worker-reasoning, model-window, and architecture follow-up passed the full Pi
suite: 2,379 tests across 211 files. The package built successfully against the
restored Awareness exports. The following whole-workspace results
precede this follow-up.

An intermediate run exposed a recovery fixture with an unrealistic 100-token
model window. Scaling its window and usage together preserved 95% pressure
without changing assertions. Another run could not resolve the built Awareness
`out/host-api.js` export; rebuilding Awareness restored both package imports.
The next run passed 2,378 tests and found a stale packaging assertion after
Awareness expanded its catalog from 21 to 25 operations. Replacing the count with
canonical inventory equality and checking the added operations passed both the
targeted test and the full suite. No dependency source, generated file, coverage
floor, or runtime assertion was weakened to obtain the passing result.

| Check | Result |
|---|---|
| Workspace build | Passed; Pi rebuilt after final source integration |
| Workspace lint and typecheck | Passed |
| Pi package tests | 211 files, 2,357 tests passed |
| Awareness and agent-contracts tests | 1,022 and 121 tests passed, respectively |
| CLI, MCP server, and VS Code tests | 952, 818, and 42 tests passed, respectively |
| Documentation and workspace health | Passed |
| Root test command | Passed with final source frozen. Tools-core's 4,262 tests passed and branch coverage reached 79.71%, above its unchanged 79.7% floor. |

An earlier root run failed the tools-core branch gate at 79.69%. The passing rerun
does not explain that variation. A separate Pi run overlapped the final confidence
parser edit and failed three new cases; the rebuilt, source-frozen run above passed
all three along with the full suite.

The architecture and this audit have no style errors or warnings. The tools and UI
references retain ten pre-existing style warnings outside the repaired paragraphs.

### Boundaries not established by these checks

Deterministic Pi provider tests exercise real host dispatch without paid model
requests. UI semantic probes use an explicit UI fixture; separate renderer tests
exercise layout and keyboard behavior. Neither establishes visual quality in every
terminal. The RPC transport probe uses a fixture extension, so it does not establish
full Octocode-over-RPC conformance.

Sampling retains the complete MCP message payload as serialized JSON in a Pi user
message; native role/media mapping remains a separate contract change. Awareness's
canonical outcome taxonomy remains `success`/`failure`. Pi retains blocked and
cancelled categories in its runtime observation but omits them from that binary
sensor. This prevents false failures without changing the public taxonomy.

Earlier validation runs intermittently failed to resolve built Pi artifacts.
Subsequent imports and reruns succeeded, but the cause remains unproven. A passing
rerun does not close that reliability finding.
