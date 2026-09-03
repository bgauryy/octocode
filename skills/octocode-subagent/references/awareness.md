# Awareness Coordination

Load when shared repository state can change EXECUTE or VERIFY. Why: peers, overlap, locks, messages, verification debt, recovery, or reusable learning must survive beyond one worker's context.

If `octocode-awareness` is available, use its normal loop: inspect relevant shared state, declare bounded work, and paths, coordinate actionable overlap, record observed checks, close the run, and reflect verified reusable learning.

Use it for concurrent editors touching related paths; shared plans, ownership, handoffs, or inbox messages; non-mergeable state needing an exclusive lock; and verification debt or durable memory that changes the plan.

Skip it for routine solo work with no shared-state signal. Ordinary overlap is advisory; lock only when simultaneous mutation is unsafe. Do not hand-edit generated coordination databases or duplicate host-projected presence calls.

If Awareness is unavailable, inventory active work when possible, assign disjoint paths, keep the parent as integration owner, and report the missing coordination evidence.

Next: return to `references/coordinate.md` while workers are live. After worker completion, load `references/completion.md`. Then verify and close.
