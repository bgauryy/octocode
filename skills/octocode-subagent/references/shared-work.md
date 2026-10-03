# Shared Work

Load when workers share a repository, cwd, or mutable files, or when peers, messages, leases, or handoffs can change EXECUTE or VERIFY. Parallel agents collide without ownership.

## Before parallel writes

1. Use `octocode-agents-communication` and its bound tools when available. Reuse the host's identity, canonical workspace, and shared database. A managed host owns presence and delivery; do not start duplicate lifecycle loops.
2. Inventory active peers and ownership.
3. Assign disjoint write paths and resolve overlapping ownership before edits.
4. Acquire the required advisory path leases. Write only after the lease succeeds.

## Rules

- Treat the shared filesystem and env-backed services as mutable.
- Put disjoint write paths and a verification command in each packet.
- Prefer read-only workers. The parent applies mutations unless ownership transfers.
- A lease is never permission to exceed the task. Resolve conflicts through a handoff or independent work.
- Send only questions, decisions, blockers, and results that change the receiver's next action. Link large handoff documents; do not repeat them. Acknowledge each delivered message after you handle it.
- Keep plans, observed checks, unresolved verification, context recovery, and lessons in host task state or notes. Reorient from those records after context loss.
- Communication stores coordination evidence. It does not replace host verification or self-regulation.
- If communication tools are unavailable: inventory active work through the host, assign disjoint paths, keep the parent as integration owner, and report the missing coordination evidence.
- Routine solo work needs no coordination setup.

Next: while workers are live, `references/coordinate.md`. After completion, `references/completion.md`; verify the integrated result and release owned leases.
