# Workspace

Load when workers share a repository, cwd, or mutable files. Why: parallel agents collide without ownership.

## Before parallel writes
1. Inventory active peers and ownership using the bound communication tools when available.
2. Assign disjoint paths and resolve overlapping ownership before edits.
3. Follow the communication skill to acquire advisory path leases before writing; proceed only after successful acquisition.
4. Send decisions, blockers, and handoffs when they change peer work; release leases when finished.

## Rules
- Treat shared filesystem and env-backed services as mutable.
- Assign **disjoint write paths** + a verification command in the packet.
- Prefer read-only workers; parent applies mutations unless ownership transfers.
- After session reload, spawn fresh workers — do not reuse stale worker ids.
- When peers, messages, leases, or handoffs change the next action, continue through `references/shared-work.md`. Keep verification and recovery notes in host task context.

Next: `packets.md` · `coordinate.md`; shared-state signal present → `shared-work.md`.
