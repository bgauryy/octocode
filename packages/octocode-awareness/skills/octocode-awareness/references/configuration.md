# Awareness Configuration

Load when choosing storage scope, hook policy, identity, or repository ownership.

## Storage and automation

Global scope stores durable Awareness state in `$OCTOCODE_HOME/awareness/awareness.sqlite3` by default. An explicit workspace policy or `--db-scope repo` selects `<workspace>/.octocode/awareness.sqlite3`; `--db-scope repo|global` is a one-call override, and `--db <path>` wins over both. Existing stores are preserved and never merged implicitly. Neither path is `$OCTOCODE_HOME/agent/agent.sqlite3` (Agent control/index) or `$OCTOCODE_HOME/agent/core.sqlite3` (Agent runtime durability). Inspect hook automation with `config show --compact`. Never copy config parsing: the runtime uses `@octocodeai/config` and `OCTOCODE_HOME`.

Other files under `<workspace>/.octocode/`, including Octocode research databases and generated projections, retain their own owners. Do not merge, rename, delete, or infer Awareness state from them.

For a recognized old mixed store, run the explicit `coordination maintenance migrate-legacy` procedure; it copies only recognized Awareness entities to the global store, refuses a nonempty target, and preserves the Agent source. Never migrate by opening the Agent database as Awareness. Ordinary scope changes do not migrate or merge existing databases.

## Identity and trust

Use one stable `OCTOCODE_AGENT_ID` per cooperating identity. Workspace paths must normalize to the same absolute root. Configuration proves preferences only; it does not prove host trust, hook execution, or model-visible delivery.

## Hook policy

| Profile | Purpose |
|---|---|
| `guard` | Block real exclusive conflicts and protect write safety. |
| `coordination` | Guard plus bounded presence and shared-state pointers. |
| `full` | Coordination plus the broadest supported lifecycle coverage. |

Hook installation mutates host configuration. Always show a noncompact dry-run immediately before applying and require explicit approval. Then install and strict-check the same host/scope. Pi uses native events and never shell-hook installation.

For first hook enablement, ask together: hook profile, host, and project/global destination. These answers are not installation approval.

Use `references/hooks.md` for lifecycle coverage and runtime smoke checks. Use `references/architecture.md` for database ownership and path normalization.

Next: return to `SKILL.md` after any requested automation change is verified.
