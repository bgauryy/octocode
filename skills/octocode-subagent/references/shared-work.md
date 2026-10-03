# Shared Work

Load when workers share a repository, cwd, or mutable files, or when peers, messages, leases, or handoffs can change EXECUTE or VERIFY. Parallel agents collide without ownership. Routine solo work needs no setup.

Before parallel writes:
1. Use `octocode-agents-communication` and its bound tools when available. Reuse the host's identity, canonical workspace, and shared database. A managed host owns presence and delivery; start no duplicate lifecycle loops.
2. Inventory active peers and ownership.
3. Assign disjoint write paths; resolve overlapping ownership by handoff or independent work before edits.
4. Acquire the required advisory path leases; write only after success.

- Treat the shared filesystem and env-backed services as mutable.
- Prefer read-only workers; the parent applies mutations unless ownership transfers.
- Keep plans, observed checks, open verification, context recovery, and lessons in host task state or notes; reorient from them after context loss.
- Communication stores coordination evidence; it does not replace host verification.
- No communication tools: inventory work through the host, assign disjoint paths, report the missing coordination evidence.

Next: while workers are live, `references/coordinate.md`. After, `references/completion.md`; verify the integrated result and release owned leases.
