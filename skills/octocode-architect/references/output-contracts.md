# Output Contracts

Load when a consequential plan or review needs an explicit decision record. Why: expose boundaries and impact without forcing ceremony on small work.

## Minimal slice

Use for low-risk, local work:

```text
Slice: <change and reason>
Impact: <scope checked>
Verify: <real check and result>
```

## Consequential plan

Include only material fields; write `N/A — <reason>` only when omission surprises readers.

```text
Slice: <smallest useful outcome>
Place/Wiring: <source → transform → boundary → sink>
In / Out: <ships> / <excluded>
Interface + invariants: <owned contract>
Dependencies/Parallel: <ordering constraints; independent work worth running concurrently>
Test + edges: <first failing surface case; absent/concurrent/replay/etc.>
Blast/Impact: <callers, data, runtime, ops, records>
Risks: <security, resilience, cost, observability>
Rollout/Revert: <migration, flag, rollback>
Rejected: <viable alternative and evidence-based reason>
```

Pause only when the model, ownership, contract, or migration choice is unsettled. Otherwise implement the named slice. <!-- style-lint: ignore-line passive-voice -->

## Review

```text
Major: <one finding or none>
Impact: <what else moves if wrong>
Housekeeping/Bookkeeping: <done, missing, or N/A>
Verification: <checks and observed results>
Verdict: block | merge-ok | approve
```

Lead with the major decision. Name the type, field, function, or interface. Avoid persona imitation, preambles, and checklist narration.

Next: during implementation load `delivery-discipline.md`; after a completed review return to `SKILL.md`.
