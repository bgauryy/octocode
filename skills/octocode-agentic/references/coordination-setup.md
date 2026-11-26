Load when installing agent communication, wiring MCP and host hooks, or repairing binding.

# Set up local agent coordination

Connect the current host to agents sharing one local SQLite database. Agents own their plans; this layer exchanges messages, data and activity status. [Choose the interface](coordination.md#choose-the-interface) and [Caller identity and binding](coordination.md#caller-identity-and-binding) own the shared MCP, init and identity rules; [Mail and evidence](coordination.md#mail-and-evidence) owns messaging outcomes and continuations.

## Install and inspect

1. Identify the actual host, workspace, existing MCP binding and hooks. Vendor names are `codex`, `claude`, `cursor`, `grok` and `octocode`; Pi binds through the package's `pi-inbox` extension, not hooks. Octocode here means the coding agent, not the research CLI. An executable named `agent` alone does not identify Cursor.
2. Reuse a working package at a stable path. If absent, use Node.js `^24.15.0` (built-in SQLite and FTS5; no additional interpreter, API key or Octocode `.env` entry is needed) and `npm install --global @octocodeai/octocode-agents-communication`. Verify `octocode-agents-communication /cli --help`. Permanent hooks must not point into an ephemeral `npx` cache.
3. Run `octocode-agents-communication /cli skill --json`. Read its `instructions`; resolve `HOST_SETUP.md` and `INSTALLATION.md` under its absolute `referenceRoot`. These installed guides own version-specific setup. Check `schema host-config` before choosing a preset; report an unsupported installed version.
4. Reuse the shared database. Default: `<OCTOCODE_HOME or ~/.octocode>/agents-communication/communication.sqlite`. Linked worktrees use the same file. Check `db info --json --workspace PATH [--database FILE]` through `/cli`. `db info` creates nothing: an absent database (`exists: false`) is created at first binding, by `join`, MCP `init` or a host hook. For incompatible databases follow [Recovery](coordination-verify.md#recovery).

## Configure host hooks and MCP

Generate the hooks through `/cli` and merge them into the host's project hook file (table below) with [hook-merge.mjs](../scripts/hook-merge.mjs), previewing first:

```bash
octocode-agents-communication /cli host-config --json --vendor HOST --workspace ABSOLUTE_WORKSPACE [--database ABSOLUTE_DB] \
  | node <skill-folder>/scripts/hook-merge.mjs --file HOOK_FILE --add - --replace 'octocode-agents-communication|communication\.js host-hook|completion-check' --dry-run
```

Read the preview's `settings` and `removed`: `--replace` must match only this installation's communication handlers, including old `completion-check` gates; narrow the regex when it removes anything else. Rerun without `--dry-run` to write. It keeps unrelated settings and handlers, skips identical entries (`skipped`), so reinstalling is idempotent, and never writes a preview file into a hook directory. Exit 1 `versionConflict` or 2 invalid JSON leaves the file unchanged.

| Host | Project hook file | Verify loading |
|---|---|---|
| Codex | `.codex/hooks.json` | `/hooks`; trusted project, hooks enabled |
| Claude Code | `.claude/settings.local.json` | `/hooks`; restart/resume |
| Cursor | `.cursor/hooks.json` | Preserve `version:1`; reload and inspect hook diagnostics |
| Grok Build | `.grok/hooks/communication.json` | From the Git project root, `grok inspect --json`: `projectRoot`, `projectTrusted`, loaded hooks |
| Octocode coding agent | `.octocode/hooks.json` | `/trust` if needed, `/reload`, `/harness hooks` |

Octocode also imports Claude/Codex hooks. Disable duplicate imported **communication handlers** in its `/harness hooks` before enabling the native preset; retain unrelated hooks. Current Octocode uses native hooks and MCP, not Pi code extensions.

Configure one stdio MCP server per agent with the stable `octocode-agents-communication` executable and `args: ["--workspace", "ABSOLUTE_WORKSPACE", "--database", "ABSOLUTE_DB"]`. Omit `--database` for the default. Merge into the host's MCP configuration following its guide; Octocode uses `.octocode/mcp.json` and `/mcp config`. The default tool profile is messaging.

Follow [Caller identity and binding](coordination.md#caller-identity-and-binding) to reuse the hook identity without hardcoding a transient session or duplicating an already-managed identity. Host trust stays with the host; report any user-only activation step precisely.
