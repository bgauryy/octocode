Load when verifying workflow work, sharing coverage, resuming, recovering, or reconciling an uncertain mutation.

# Verification and recovery

The orchestrator is the sole ledger writer. Actors return raw observations keyed to its row; neither maintains a competing ledger nor accepts work. Reuse an existing task ledger; otherwise keep `coverage.md` at `<workspace>/.octocode/octocode-workflows/tmp/<YYYY-MM-DD>-<slug>/`.

## Automatically check artifact identity

For a runtime claim, use the project's existing build and acceptance harness to assert identity before replay and acceptance.

1. Capture expected authored inputs (including relevant uncommitted work), generated outputs, artifact digests or immutable IDs, and source/build correspondence before and after the required pipeline. Detect a source/input/output replacement during build, activation, replay, or acceptance; it stales the result.
2. Give one owner the affected build outputs and activation. Reuse a completed matching build or join an active one; do not race builds or reloads. Keep relevant inputs and outputs stable through build, activation, replay, and acceptance; if concurrent replacement is detected, invalidate the affected row before it can be accepted.
3. Compare each changed loaded component with its supported build/health/introspection identity. If that surface is unavailable, controlled fresh activation may establish identity only by stopping the owned old process, verifying resolved artifacts including dynamic dependencies, launching the exact process on a bound endpoint, and preventing artifact replacement through replay. A disk hash, PID, path, version, reload request, or new launcher alone cannot prove an already loaded child or addon changed; do not invent missing introspection.
4. Mismatch or unknown identity blocks acceptance. Mark the runtime row `unverified`, name missing proof, and continue independent work. Source-only checks record source/index freshness and cannot prove runtime behavior.
5. Bind identity result, relevant non-secret configuration, fixture, state/reset conditions, and acceptance result to the row. Do not record credentials or their hashes.

Process-session handoff is owned by [Runtime surgery's lifecycle rule](runtime-surgery.md#process-lifetime-and-handoff), not by this identity guard.

## Keep one compact coverage ledger

| Key / surface → boundary / scenario | Acceptance check | Evidence: source + build/conditions + result | State | Gap or next check |
|---|---|---|---|---|

Use exactly these states: `queued` (discovered), `verified` (current named check passed), `failed` (valid check contradicted acceptance), `unverified` (prerequisite blocked a valid check), and `stale` (earlier evidence no longer covers current conditions). A runtime `verified` row includes the identity guard.

After an owning/shared-boundary/source change, stale dependent scenarios and callers/consumers; after generated-contract change, stale affected adapters/serialization checks; after relevant configuration or fixture/state change, stale dependent scenarios/comparisons. Retain independent evidence only with a stated dependency basis. On resume, reconcile the ledger with current source, artifacts, configuration, and state; do not create a second ledger.

## Diagnose and replan from evidence

Keep each diagnosis a hypothesis test: claim, strongest rival, distinguishing check, result (confirmed, dismissed, unresolved). Do not repeat unchanged edit/build/restart/request merely because it failed; a retry after a reasoning failure needs a revised hypothesis.

| Evidence | Next action |
|---|---|
| Stale artifact, wrong endpoint, partial build, contract mismatch | Repair owner build/activation, rerun identity guard, replay same acceptance |
| A command reported success but the artifact's timestamp or digest did not change | The command did not run as intended (shell quoting, wrong script, no-op target). Check the artifact, never the exit code alone |
| A before/after comparison would measure a target that is still changing | Freeze a copy of the measured artifact before the change and measure both sides against fixed inputs |
| Wrong fixture/assertion/configuration/state | Establish owning contract and intended setup; preserve a check for the original defect |
| Product defect on intended build | Trace owning boundary, make smallest authorized correction, rebuild, invalidate/replay dependent rows |
| Ambiguous cause or partial semantic evidence | Refresh context, narrow claim, run a discriminating probe, preserve disproof |
| Transient external failure | Bounded retry, only with evidence of transience and the operation's replay/deduplication contract |
| Missing access, actor, dependency, or authorization | Record prerequisite, request it through the proper owner as an SBAR (situation, background, assessment, recommendation), continue independent in-scope rows |

A timeout is an unknown mutation outcome: inspect durable state and idempotency/operation identity before replay. If effect remains unknown, leave it unverified. Continue while a new observation can progress within scope and budget; stop repeating the failed attempt, not useful work. Browser mutation safeguards and sealed-evaluation integrity are stricter domain limits and remain in force.

Next: return row evidence to the orchestrator for merge and domain output.
