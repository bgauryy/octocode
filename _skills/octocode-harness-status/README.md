# Octocode Harness Status

Inventory the AI harness on this machine: installed skills, MCP servers, CLIs, and token/context signals — then clean up from a dashboard when needed.

Use when you need to see what agents can actually load, or remove stale MCP/skill installs.

## Capabilities

- List skills, MCPs, and CLIs per vendor/host
- Open an interactive HTML dashboard in the browser
- Review agent context / token pressure signals
- Remove MCP config entries or skill folders from the UI

## How It Works

```text
SCAN HOST CONFIGS → RENDER DASHBOARD → OPTIONAL CLEANUP
```

The skill reads local host configs, presents an inventory, and only mutates after explicit UI actions. It does not invent installs — it reports what is present.

## Installation

```bash
npx octocode skill --name octocode-harness-status
```

## Maintainer Notes

Keep this README user-facing. Implementation details stay in `SKILL.md` and scripts. Prefer dry-run/preview before destructive cleanup paths.
