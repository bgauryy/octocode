# @octocodeai/pi-extension

<div align="center">
<img src="https://github.com/bgauryy/octocode-mcp/raw/main/packages/octocode-pi-extension/assets/logo.png" width="640px" alt="Octocode + Pi">
</div>

Octocode’s research tools, Awareness coordination, system prompt, skills, web
providers, and subagents as one Pi extension.

```bash
pi install npm:@octocodeai/pi-extension
/octocode-status
```

The build bundles both command lines:

- `$OCTOCODE_CLI` → `node "$OCTOCODE_CLI" <command>`
- `$OCTOCODE_AWARENESS_CLI` → `node "$OCTOCODE_AWARENESS_CLI" <noun> <verb> --compact`

## What loads

| Surface | Count |
|---|---:|
| Native Octocode research tools | 13 |
| Pi support tools | 7 |
| Replacement edit tool | 1 |
| Slash commands | 6 |
| Bundled main-agent skills | 7 |

Awareness memory and coordination are deliberately not Pi tools. Agents use the
bundled CLI under the `octocode-awareness` skill, while in-process hooks automate
file presence, exclusive-conflict checks, briefings, and finish warnings. This
keeps one CLI/schema contract instead of duplicating it in Pi tool definitions.

## Quick start

```text
/octocode-status         health and configured surfaces
/octocode-harness        exact live tools, commands, and skills
/octocode-setup          manage project .pi/APPEND_SYSTEM.md
```

At the start of coding work, the agent should run:

```bash
node "$OCTOCODE_AWARENESS_CLI" attend --workspace "$PWD" --compact
```

It then chooses a ready task or opens standalone advisory Work:

```bash
node "$OCTOCODE_AWARENESS_CLI" work start \
  --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --file src/a.ts \
  --rationale "fix parser" --test-plan "parser tests" --compact
```

Ordinary file presence is advisory and can overlap. Add `--exclusive` only for
sensitive/non-mergeable work. Finish the exact owned run after its stated check:

```bash
node "$OCTOCODE_AWARENESS_CLI" work end \
  --agent-id "$OCTOCODE_AGENT_ID" --run-id run_123 --compact
node "$OCTOCODE_AWARENESS_CLI" verify mark \
  --agent-id "$OCTOCODE_AGENT_ID" --run-id run_123 \
  --message "parser tests passed" --compact
```

Do not batch-verify another agent’s work. See
[docs/MEMORY_AGENT_FLOW.md](docs/MEMORY_AGENT_FLOW.md) and
[docs/REFLECT.md](docs/REFLECT.md).

## Native Octocode tools (13)

These execute directly through `@octocodeai/octocode-tools-core`; no MCP process
or duplicate interface layer is started:

| Area | Tools |
|---|---|
| GitHub | `ghSearchCode`, `ghSearchRepos`, `ghHistoryResearch`, `ghGetFileContent`, `ghViewRepoStructure`, `ghCloneRepo` |
| Local | `localSearchCode`, `localFindFiles`, `localGetFileContent`, `localViewStructure`, `localBinaryInspect` |
| Semantics | `lspGetSemantics` |
| Packages | `npmSearch` |

Pi’s built-in `read`, `grep`, `find`, and `ls` are disabled in favor of the
corresponding Octocode tools. The extension replaces Pi’s `edit` tool with a
batch-aware implementation that verifies all matches before writing and exposes
stale-read diagnostics.

## Support tools (7)

| Tool | Purpose |
|---|---|
| `web` | Search provider adapter. |
| `chromeDebug` | Chrome DevTools Protocol operations. |
| `browserAgent` | Multi-turn browser subagent. |
| `spawnSubagent` | Spawn a declared packaged subagent. |
| `manage_context` | Inspect and compact Pi context. |
| `spawnAgent` | Start a background Pi worker. |
| `AgentMessage` | List, message, steer, wait for, abort, or kill workers. |

Awareness commands such as `attend`, `task ready`, `work start`, `signal list`,
`memory recall`, and `reflect record` are invoked through
`$OCTOCODE_AWARENESS_CLI`, not registered again as tools.

## Slash commands (6)

| Command | Purpose |
|---|---|
| `/octocode-status` | Health, prompt, skills, memory home, and providers. |
| `/octocode-harness` | Exact registered surface inventory. |
| `/octocode-setup` | Install/update the managed system-prompt block; `--global` targets user scope. |
| `/octocode-skills-update` | Refresh bundled skill installs. |
| `/octocode-memory-digest` | Preview report-first cleanup; `--apply` requires confirmation. |
| `/octocode-memory-forget` | Preview targeted memory deletion; `--apply` requires confirmation. |

The two memory maintenance commands are user controls. Agent-facing recall,
recording, signals, tasks, verification, and reflection stay on the Awareness CLI.

## Bundled skills (7)

Pi discovers one generated skill tree under `dist/skills/`:

- `octocode-awareness`
- `octocode-brainstorming`
- `octocode-prompt-optimizer`
- `octocode-research`
- `octocode-rfc-generator`
- `octocode-roast`
- `octocode-skills`

`octocode-awareness` is copied from the Awareness package’s canonical skill.
The Pi build owns the generated copy; never edit it by hand.

## Awareness bridge

The bridge derives one identity per Pi session unless the user explicitly sets
`OCTOCODE_AGENT_ID`. `/new`, `/resume`, forks, and sequential sessions therefore
cannot inherit a previously derived agent identity. Both hooks and child CLI
commands see the current identity through the environment.

Before a write/edit tool call, hooks attach the path to the active Task/Work run
or create visible fallback work. Another agent’s exclusive lease blocks the edit;
ordinary advisory overlap stays visible and allowed. On conclusion, the bridge
warns about exact owned pending work rather than silently declaring success.

Pi wires this bridge in process. Do not install shell hook files for Pi.

## System prompt

Authored sections live under `src/prompts/sections/` and build to
`dist/system/SYSTEM_PROMPT.md`. They define authority, operating mode, Awareness,
tool routing, research evidence, skills, code discipline, and safety. Managed
prompt installation is marker-based and idempotent.

## Configuration

The extension loads Octocode configuration through `@octocodeai/config`.

| Variable | Purpose |
|---|---|
| `OCTOCODE_AGENT_ID` | Optional explicit stable Awareness identity. |
| `OCTOCODE_MEMORY_HOME` | Awareness database directory. |
| `GITHUB_TOKEN`, `GH_TOKEN`, `OCTOCODE_TOKEN` | GitHub authentication. |
| `ENABLE_LOCAL` | Enable local research tools (default on). |
| `ENABLE_CLONE` | Enable `ghCloneRepo`. |
| `TAVILY_API_KEY`, `SERPER_API_KEY` | Higher-quality web providers. |
| `OCTOCODE_NO_VERIFY_GATE=1` | Emergency bypass for a misfiring finish gate. |

Never put secrets into prompts, logs, Awareness memory, or committed config.

## Troubleshooting

| Symptom | Action |
|---|---|
| Extension appears inactive | Run `/octocode-status`, then `/octocode-harness`. |
| Awareness command missing | Check `$OCTOCODE_AWARENESS_CLI` exists and run `node "$OCTOCODE_AWARENESS_CLI" schema commands --compact`. |
| Finish warning remains | Run the stated test, `verify audit` for your agent, then `verify mark --run-id <exact-run>`. |
| Stale dead-session work | Audit exact ownership and explicitly abandon only after review. |
| Local research tools absent | Check `ENABLE_LOCAL`. |
| Clone tool absent | Set `ENABLE_CLONE=1`. |

## Development

```bash
yarn workspace @octocodeai/pi-extension typecheck
yarn workspace @octocodeai/pi-extension test
yarn workspace @octocodeai/pi-extension check:no-workspace
```

The last gate intentionally fails while repository dependencies use `workspace:*`;
the release sync must pin published semver versions before packaging.
