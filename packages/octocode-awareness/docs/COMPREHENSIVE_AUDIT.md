# Comprehensive Awareness Audit

Use this scored lane to answer whether the complete Awareness control loop works.
Run in a disposable workspace/database. Complete the Installed and applicable Host
lanes in [VERIFY.md](VERIFY.md), then run the isolated setup and commands in
[FEATURE_SWEEP.md](FEATURE_SWEEP.md). A `BLOCKED` row names its prerequisite and
next command; it is never omitted. Pipe `< /dev/null` into every call and set env
vars inline per command — an open stdin hangs instead of failing, and a dropped
`export` silently reads the wrong store.

| Area | Evidence | Score 4–5 requires |
|---|---|---|
| Store and schema | self-test; fresh initialization; schema tests; rejected foreign/drifted store | branded, integrity/FK/fingerprint checks pass; incompatibility fails closed |
| Reads | compact attend/status; targeted work/verify/signal/memory reads | bounded next action, explicit omissions, deliberate deep drill-down |
| Work | two-agent same-file start/list/show/heartbeat/end | peers coexist and ending one run preserves the other |
| Locks | conflicting acquire/wait/release; stale dry-run prune | conflict exits 2 before unsafe presence; cleanup never means success |
| Tasks and verify | dependency block/unblock; submit/mark/audit | predecessor stays blocked until verified; zero final debt |
| Communication | register; signal publish/list/reply/ack/resolve; refinement lifecycle | recipient/thread/unread ownership holds; unrelated work is preserved |
| Memory and reflection | record/recall/supersede/archive/restore; reflect record | scoped/ranked SQLite truth; immutable replacement history; current evidence wins |
| Wiki | sync; manifest; source-revision comparison; local/share review | bounded/stale-aware projection; `manifest.source.canonical` matches the run's real `db_path`; live work is not treated as wiki truth |
| Hooks | strict check; harmless observed host event; lifecycle edge | config ready and runtime observed; guard/presence/audit/finalize parity |
| Skill | docs list/show; bundled-skill inspection; skill review | live-state routing, manual fallback, no secret handling, focused owners |
| Token delivery | compact tests; unchanged repeat; changed signal/peer/memory lead | stable state silent; one bounded decision packet; full details queryable |
| Maintenance | digest; stale previews; final audit | sensors do not delete live work; reviewed cleanup; no locks/debt |

## Scoring

| Score | Meaning |
|---:|---|
| 5 | Normal and adverse behavior passed with repeatable, bounded evidence. |
| 4 | Contract passed; only a minor usability/observability gap remains. |
| 3 | Happy path passed, but an edge/recovery/trust proof is missing. |
| 2 | Important lifecycle or control boundary is unproven or failed. |
| 1 | Surface starts, but its claimed coordination contract is not demonstrated. |
| 0 | Behavior fails, is unsafe, contradicts canonical state, or lacks usable evidence. |
| BLOCKED | A named required prerequisite is unavailable. |

Do not average away safety or authority failures. **PASS** requires every applicable
row at 4–5, a mean of at least 4.5, no blocked rows, and zero final debt/active
locks. **PARTIAL** requires every safety/authority row (store/schema, locks,
verification, wiki authority, and applicable hooks) at least 4 but misses the
overall score threshold. **FAIL** is any safety/authority row at 0–3, a failed
required behavior, or final debt/active locks. **BLOCKED** prevents a comprehensive
verdict.

For token cost, compare bytes of the initial compact JSON read with the unchanged
repeat. Require a bounded initial decision packet, no repeated unchanged delivery,
truncation metadata where needed, and targeted full-row retrieval. The tested
`attend --compact` contract is 2 KiB; do not replace it with a row-count proxy.

## Receipt

```text
Comprehensive audit: PASS | PARTIAL | FAIL | BLOCKED
Ratings: store=<0-5|blocked> reads=<0-5|blocked> work=<0-5|blocked> locks=<0-5|blocked> verify=<0-5|blocked> comms=<0-5|blocked> memory=<0-5|blocked> wiki=<0-5|blocked> hooks=<0-5|n/a|blocked> skill=<0-5|blocked> tokens=<0-5|blocked> maintenance=<0-5|blocked>
Score: mean=<0.0-5.0> low_rows=<none or area=score list>
Debt: pending=<count> active=<count> locks=<count>
Blocked/skipped: <none or exact prerequisite + next action>
Evidence: <db_path, workspace path, test/command citations per row>
```
