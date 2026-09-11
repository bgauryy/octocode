# Output Contracts

Load when a consequential plan or review needs an explicit decision record. Why: expose boundaries and impact without forcing ceremony on small work.

For low-risk local work:

```text
Slice: <change and reason>
Impact: <scope checked>
Verify: <real check and result>
```

## Consequential plan

Include only material fields; write `N/A — <reason>` only when omission surprises readers.

```text
Slice: <smallest useful outcome>
Place/Wiring: <external/internal source → transform → boundary → sink/egress>
In / Out: <ships> / <excluded>
Interface + invariants: <owned contract>
Contract + data flow: <provider/consumer, runtime validation, trust/persistence/egress boundaries>
Test + edges: <first failing surface case; absent/concurrent/replay/etc.>
Blast/Impact: <callers, data, runtime, ops, records>
Rollout/Revert: <migration, flag, rollback>
Rejected: <viable alternative and evidence-based reason>
```

## Review

```text
Major: <one finding or none>
Impact: <what else moves if wrong>
Housekeeping/Bookkeeping: <done, missing, or N/A>
Verification: <checks and observed results>
Verdict: block | merge-ok | approve
```

For architecture findings, add only decision-changing fields:

```text
Model: <owners, allowed arrows, representative flow>
Finding: <expected boundary → mechanism → impact → exact proof>
Alternate/Confidence: <killed or unresolved> / confirmed | likely | candidate | dismissed
Refactor: <quality attribute, seam, vertical slices, preserved contract, rollback>
Hot path: <workload/budget, end-to-end baseline, attributed cost, comparable result>
```

For an interface or tool contract, review the complete path: input schema → actual adapter arguments → result shape and evidence → executable `next` continuation. Check one valid and one invalid example for each changed branch, distinguish static types from runtime validation, and separate measured reliability from an unmeasured expectation. For a multi-tool surface, compare shared field names and meanings across the set and run at least one held-out composition case.
Next: during implementation load `delivery-discipline.md`; after a completed review return to `SKILL.md`.
