# Awareness

Load when workers share a repo or may write. Why: parallel agents share cwd + Awareness DB.

## Before parallel writes
1. `attend --compact` — Ready/Claimed/Verify/FilesUnderWork.
2. Claim matching plan task or open standalone work.
3. Declare advisory file presence for every edit path.
4. Use exclusivity only for sensitive paths.

## Identity
Each Pi worker gets Awareness id:
`{OCTOCODE_AGENT_ID|pi-agent}:worker:{uuid8}`

The Awareness worker id differs from the spawn `agentId` UUID. Spawn a fresh worker after session reload — do not reuse either id.

## Prefer live over wiki
SQLite is canonical. Generated `.octocode/*.md` are leads — prefer `attend` / `work` / `query` / `memory recall`.

Full protocol: load `octocode-awareness`.

Next: `packets.md` · `pi-runtime.md`.
