# Communication benchmarks

Measured on macOS ARM64 on 2026-09-25. These are local development measurements,
not guarantees for other machines, provider loads or coding tasks.

## Reproduce

From the repository root, build the embedded skill before measuring:

```sh
yarn workspace @octocodeai/octocode-agents-communication build:release
yarn workspace @octocodeai/octocode-agents-communication benchmark:runtime
COMMUNICATION_PI_MODEL='PROVIDER/MODEL' \
  yarn workspace @octocodeai/octocode-agents-communication benchmark:scheduling
node packages/octocode-agents-communication/scripts/lease-crash-poc.mjs
```

The runtime benchmark uses no models. The scheduling benchmark makes live vendor
calls and requires authenticated Codex, Claude and Pi CLIs. Choose a configured Pi
model explicitly. Both scripts save manifests and raw results under
`.octocode/benchmarks/`; scheduling freezes the actual CLI-served skill, not just
the Markdown source. It rejects missing usage evidence instead of reporting zero.

## Matched scheduling comparison

Twelve fresh workers: two Codex Luna, two Claude Haiku and two Pi Haiku workers
per arm. Within each pair, task text, tools, skill and the six-second interval
between FYI and PROCESS were identical. Only the FYI wake policy differed.
Order was baseline→candidate for one pair and candidate→baseline for the other.

Every baseline used two handling turns; every passive candidate used one. All
twelve acknowledged both message IDs and sent exactly one requested reply. All
six passive candidates had zero model usage, delivery batches and completed turns
before PROCESS. No duplicate delivery appeared in the one-second follow-up window.

| Vendor | Input tokens: immediate → passive | Reduction | Median PROCESS completion: immediate → passive |
| --- | ---: | ---: | ---: |
| Codex | 108,210 → 64,754 | 40.2% | 5.296 → 9.537 s |
| Claude | 73,326 → 29,361 | 60.0% | 7.345 → 7.864 s |
| Pi | 46,160 → 23,280 | 49.6% | 3.734 → 4.146 s |

Token totals combine the two workers per arm and subtract each worker's observed
startup. Input includes cache reads/writes; cached reads are a subset, not extra
input. Codex cumulative snapshots are differenced, never summed. These numbers
are cumulative request inputs, not maximum context size or monetary savings.

Batching saved turns and recorded input for this workload, but every observed
completion median increased. It moves FYI handling into the actionable turn.
The predeclared latency guard allowed twice the baseline plus two seconds and
passed; it does not establish a speed improvement. Use passive messages for
information that can wait, and actionable messages for authorized requests.

Verdict: **KEEP, exploratory**. Two pairs per vendor, shared/reachable host storage,
paired databases, uncontrolled provider caches and concurrent host work prevent
statistical or held-out claims. This tiny messaging task does not measure coding
quality or operation near a context limit. Completed turns are not individual
model-request counts.

An earlier complete run was **REVISE**: two baseline workers sent unsolicited
READY messages. The final run clarified the initial plain assistant response and
used the skill's rule against unsolicited startup announcements in both arms.
It isolates wake policy within the final run; it does not prove which wording
change improved startup behavior.

Evidence directories:

- Final: `.octocode/benchmarks/communication-scheduling/results/2026-09-25T12-30-06.020Z-8495d8b1/`
- Earlier failures: `.octocode/benchmarks/communication-scheduling/results/2026-09-25T12-24-28.474Z-9638be72/`

Each contains the frozen runner/contract, served skill, provider versions, prompt
hashes, raw traces and an assessment. All final worker processes exited; all
24 startup/final telemetry samples were complete and subject hashes stayed fixed.

## Model-free runtime and leases

The final release-binary measurement includes process/launcher startup and DB
work, with 40 samples per operation. Concurrent repository tests were running;
this is a descriptive measurement, not a performance regression threshold.

| Operation | Median | p95 |
| --- | ---: | ---: |
| Store a message | 28.76 ms | 37.32 ms |
| Deliver through a raw hook | 26.67 ms | 33.68 ms |
| Empty hook | 25.73 ms | 31.64 ms |
| Acquire a lease | 29.02 ms | 35.25 ms |
| Check a lease conflict | 30.47 ms | 34.33 ms |

All 40 identical send retries reused their message IDs. Hooks offered each message
once; all 40 idle hook responses contained zero bytes. All deliveries were
acknowledged. Twenty opposing two-path lock attempts produced exactly one complete
winner each, with no partial reservations or remaining leases.

The real SIGKILL probe recovered a two-minute lease after **60.064 seconds**, when
its owner's presence expired. It retained the conflict before expiry and rejected
stale renewal, including after the old owner resumed. Advisory leases still cannot
fence a process that ignores the protocol; shell target extraction is best effort.

Runtime evidence: `.octocode/benchmarks/communication-runtime/results/2026-09-25T12-30-22.784Z/`.
The first harness attempt incorrectly queried a nonexistent release column; that
failed artifact is retained. The corrected guard checks the actual lease table.
Crash evidence: `packages/octocode-agents-communication/out/lease-crash-poc.json`.

The final Pi inbox adapter also passed 20 real SDK restart assertions across four
processes, with zero model starts. Fresh sessions kept messages staged until an
on-disk receipt existed; established sessions recovered identity and did not
reinject messages. Evidence:
`.octocode/benchmarks/pi-durability/results/2026-09-25-lifecycle/`.

See [retirement and trigger policy](../../../docs/COMMUNICATION_RETIREMENT.md) for
Pi persistence limits, migration, retained features and the validation summary.


## OpenCode and ACP integration

[OpenCode evaluation](OPENCODE_EVALUATION.md) records authenticated existing-session
messages, passive insertion, native MCP replies, broadcasts, and per-request usage.
[ACP evaluation](ACP_EVALUATION.md) records capability negotiation, history-free
resume, preserved supplied MCP configuration, and an explicit recipient prompt.
ACP remains a maintainer probe; the production OpenCode transport uses HTTP.

A [release service run](../../../.octocode/benchmarks/communication-service/results/2026-09-25T15-16-34.960Z/result.json)
used 60 samples and recorded no lost messages, duplicate IDs, or model calls:

| Send path | Median | p95 |
| --- | ---: | ---: |
| Persistent MCP | 0.282 ms | 0.443 ms |
| Fresh CLI process | 12.436 ms | 13.545 ms |

These timings stop at storage acknowledgement. They exclude recipient inference
and do not establish a before/after speedup. The OpenCode adapter's connection
reuse has a separate deterministic guard: two batches use one HTTP connection,
each batch checks workspace/status, and empty inbox iterations make no HTTP calls.
An [earlier service run](../../../.octocode/benchmarks/communication-service/results/2026-09-25T15-12-54.075Z/result.json)
timed out during initial registration before collecting samples; it supplies no
latency evidence and remains recorded separately.

## Hook, recovery and storage improvements

- [Hook comparison](HOOK_EVALUATION.md): idle events avoid writes and remain responsive under a held SQLite writer.
- [Recovery evaluation](RECOVERY_EVALUATION.md): repeated busy, disconnect and listener restart fixtures distinguish explicit retries from unexpected duplicates.
- [Retention and compaction](RETENTION.md): reclaim reusable pages while preserving audit and idempotency.
- [Context and collaboration evaluation](CONTEXT_OPTIMIZATION.md): compact skill, measured prompt surface and real shared-file handoff.

`schema tools` returns descriptors alone. `--tools` fixes an optional subset for
MCP/managed workers; native Pi accepts `tools` in its binding. Serialized descriptor
bytes are 12,625 for 13 tools, 3,128 for `peers,send_message,ack`, and 3,986 when adding
`read_document` (75.2% and 68.4% smaller). These are UTF-8 JSON byte counts, not billed
tokens, total host context or a measured cache-hit improvement. CLI help remains
available for omitted capabilities; the selected MCP endpoint refuses omitted calls.

The final four-tool native matrix passed on release binary
`48dd4eaea73300fce2acbb7e02c5a22879dfad1b1c6b5d916f1abd1799a8a00b`
and the retained 6,571-byte skill. Two recipients each from Claude, Codex, Grok,
Pi and OpenCode plus a raw client completed 110 directed questions and 110
correlated replies, read the shared document in all ten native recipients, and
acknowledged the eleven-recipient broadcast. The DB snapshot contains 232 messages
and 242 acknowledged deliveries, with no pending deliveries or duplicate replies.
Passive mail started no turns; routing made no model calls. The question round
took 179.57 seconds, including a slow OpenCode turn; this is not a speed comparison.
All owned child processes exited.

Evidence: `.octocode/benchmarks/communication-service-mesh/results/2026-09-25T15-59-20.619Z/`.
The final runtime passed 188 JavaScript/CLI tests with zero skips, 30 Rust tests,
strict Clippy, formatting, standalone skill validation, packaging and docs checks.
Verification and retained failures: `.octocode/benchmarks/communication-improvements/verification/result.json`.

Earlier attempts are retained: an OpenCode startup timeout, an unsolicited
acknowledgement message, and a reply-prefix typo. The delivery instruction now
names the `ack` call explicitly; the harness detects unsolicited controller mail
and records reply-format failures after completing the remaining protocol checks.
The final run passed both gates; that single confirmation does not prove improved
model compliance. [Editing compliance](CONTEXT_OPTIMIZATION.md) still failed, and
[fresh-copy macOS loader stalls](MACOS_STARTUP.md) remain unresolved.

## Production hardening review — September 25, 2026

See [production readiness](PRODUCTION_READINESS.md) for the current decision,
layer ratings, final artifact, atomic reply/ACK comparison, admission-guard proof,
and retained live response-quality failure. Automated success is not a blanket
production approval or a guarantee that every model follows the conversation rules.
