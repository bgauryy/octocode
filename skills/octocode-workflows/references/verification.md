Load when verifying workflow work, sharing coverage, resuming, recovering, or reconciling an uncertain mutation.

# Verification and recovery

The orchestrator is the sole ledger writer. Actors return raw observations keyed to its row; neither maintains a competing ledger nor accepts work. Reuse an existing task ledger; otherwise keep `coverage.md` at `<workspace>/.octocode/octocode-workflows/tmp/<YYYY-MM-DD>-<slug>/`.

## Automatically check artifact identity

For a runtime claim, use the project's existing build and acceptance harness to assert identity before replay and acceptance.

- Establish that the intended source/build and relevant configuration produced the component being exercised. Reuse supported build, health or introspection evidence instead of requiring a separate hash/metadata inventory.
- Give build outputs and activation one owner; reuse matching builds or join an active build. Detect replacement during build, activation or replay and invalidate affected evidence.
- If introspection is unavailable, a controlled fresh activation may establish identity: stop the owned old process, resolve its artifacts and dynamic dependencies, launch the intended component and prevent replacement through replay. A disk hash, PID, path, version or reload request alone does not prove that an already loaded component changed.
- Missing or mismatched identity leaves the runtime result unverified. Name the gap, continue independent work and retain the conditions needed to interpret the eventual acceptance check. Never record credentials or their hashes.

Process-session handoff is owned by [Runtime surgery's lifecycle rule](runtime-surgery.md#process-lifetime-and-handoff), not by this identity guard.

## Keep one compact coverage ledger

| Surface or scenario | Acceptance check | Deciding evidence and relevant conditions | State | Depends on | Gap or next action |
|---|---|---|---|---|---|

Use the existing tracker; a `coverage.md` ledger keeps this table, and a `.jsonl` ledger keeps one object per row keyed by these headings.

- **Surface or scenario:** the resolved identity plus the bounded configuration/scenario identity, such as `src/api.ts:handle · empty query · default config`. Rows with the same name are duplicates.
- **State:** `queued` (discovered), `verified` (current named check passed), `failed` (valid check contradicted acceptance), `unverified` (prerequisite blocked a valid check), or `stale` (earlier evidence no longer covers current conditions). A runtime `verified` row includes the identity guard.
- **Depends on:** the dependency basis, `;`-separated: the rows and inputs its evidence relies on. Callers and consumers name the owning row; adapters and serialization checks name the generated contract; scenarios and comparisons name their configuration and fixtures. An empty cell claims the evidence relies on nothing that can change.

After merging actor observations, and on resume, check the ledger with [ledger.mjs](../scripts/ledger.mjs): `node <skill-folder>/scripts/ledger.mjs <coverage.md>`. Exit 0: every state is valid, and `counts` holds the per-state totals for the report. Exit 1: correct each row in `invalidStates`. Exit 2: the error names the unreadable line, missing column or empty row name.

Right after an owning/shared-boundary/source, generated-contract, configuration or fixture/state change, before rechecking anything, run `node <skill-folder>/scripts/ledger.mjs <coverage.md> --stale-after "<changed row or input>" --write` (repeat `--stale-after` for each changed name). It sets the changed row and every row whose Depends on chain reaches it from `verified` or `failed` to `stale`, and lists them in `staleMarked` with the `via` chain: requeue those checks. Rows it does not reach keep their evidence. Exit 2 `unknown-name`: no row or Depends on entry has that name; correct the name or the missing Depends on entries, then rerun. On resume, run `--stale-after` for each input that changed since its rows were checked; do not create a second ledger.

## Diagnose and replan from evidence

Keep each diagnosis a hypothesis test: claim, strongest rival, distinguishing check, result (confirmed, dismissed, unresolved). Do not repeat unchanged edit/build/restart/request merely because it failed; a retry after a reasoning failure needs a revised hypothesis.

| Evidence | Next action |
|---|---|
| Stale artifact, wrong endpoint, partial build, contract mismatch | Repair owner build/activation, rerun identity guard, replay same acceptance |
| A command reported success without evidence that it produced the intended artifact | Check the target and loaded artifact. Unchanged bytes may be a valid no-op; establish whether the intended source was included before diagnosing a product defect. |
| A before/after comparison would measure a target that is still changing | Freeze a copy of the measured artifact before the change and measure both sides against fixed inputs |
| Wrong fixture/assertion/configuration/state | Establish owning contract and intended setup; preserve a check for the original defect |
| Product defect on intended build | Trace owning boundary, make smallest authorized correction, rebuild, invalidate/replay dependent rows |
| Ambiguous cause or partial semantic evidence | Refresh context, narrow claim, run a discriminating check, preserve disproof |
| Transient external failure | Bounded retry, only with evidence of transience and the operation's replay/deduplication contract |
| Missing access, actor, dependency, or authorization | Record prerequisite, request it through the proper owner as an SBAR (situation, background, assessment, recommendation), continue independent in-scope rows |

A timeout is an unknown mutation outcome: inspect durable state and idempotency/operation identity before replay. If effect remains unknown, leave it unverified. Continue while a new observation can progress within scope and budget; stop repeating the failed attempt, not useful work. Browser mutation safeguards and sealed-evaluation integrity are stricter domain limits and remain in force.

Next: return row evidence to the orchestrator for merge and domain output.
