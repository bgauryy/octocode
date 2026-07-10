# Workspace

Load when workers share a repo, cwd, or mutable files. Why: parallel agents collide without ownership.

## Before parallel writes
1. Inventory active work / locks if the host or Awareness provides them.
2. Claim or declare the task so peers can see it.
3. Announce every edit path (advisory presence).
4. Use exclusive locks only for sensitive paths.

## Rules
- Treat shared filesystem and env-backed services as mutable.
- Assign **disjoint write paths** + a verification command in the packet.
- Prefer read-only workers; parent applies mutations unless ownership transfers.
- After session reload, spawn fresh workers — do not reuse stale worker ids.

## Octocode Awareness (optional)
IF `octocode-awareness` is installed THEN use `attend` / locks / signals for shared-repo coordination. Full protocol lives in that skill — this ref only covers spawn-time collision rules.

Next: `packets.md` · `coordinate.md`.
