# Workspace

Load when workers share a repository, cwd, or mutable files. Why: parallel agents collide without ownership.

## Before parallel writes
1. Inventory active work / locks if the host provides them.
2. Claim or declare the task so peers can see it.
3. Announce every edit path (advisory presence).
4. Use exclusive locks only for sensitive paths.

## Rules
- Treat shared filesystem and env-backed services as mutable.
- Assign **disjoint write paths** + a verification command in the packet.
- Prefer read-only workers; parent applies mutations unless ownership transfers.
- After session reload, spawn fresh workers — do not reuse stale worker ids.
- When peers, messages, locks, verification debt, or durable memory change the next action, continue through `references/awareness.md`.

Next: `packets.md` · `coordinate.md`; shared-state signal present → `awareness.md`.
