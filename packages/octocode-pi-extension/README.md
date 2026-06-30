# @octocodeai/pi-extension

The official Octocode Pi package. This is the Octocode team's preferred and
recommended harness for coding agents that use Pi: keep Pi lean, then add the
Octocode system prompt, skills, setup commands, and evidence-first operating
model as one package.

Use it when you want a Pi agent to research with evidence, edit conservatively,
preserve crucial context before compaction, and verify work before claiming it
is done.

```text
orient -> search/read exact evidence -> decide or plan -> patch when asked
       -> verify -> remember or hand off what matters
```

Pi is the primary recommended host because it is a minimal terminal coding
harness extended through skills, extensions, prompt templates, themes, and
packages. Pi edits and orchestrates; Octocode supplies research, planning,
review, memory, and verification discipline.

## Install

Install from npm:

```bash
pi install npm:@octocodeai/pi-extension
```

Install from this repository while developing:

```bash
yarn workspace @octocodeai/pi-extension build
pi install /Users/guybary/Documents/octocode-mcp/packages/octocode-pi-extension
```

Project-local install:

```bash
pi install -l npm:@octocodeai/pi-extension
```

Pi packages run with full system access. Review this package before installing
it in a sensitive environment.

## What It Adds

The package build creates a self-contained Pi package:

| Asset | Source | Shipped location |
|---|---|---|
| System prompt | `packages/octocode-pi-extension/docs/PI/APPEND_SYSTEM.md` | `dist/system/APPEND_SYSTEM.md` |
| Skills | root `skills/` copied during build | `skills/` and `dist/skills/` |
| Extension | `src/index.js` | `dist/index.js` |

The extension:

- appends the bundled Octocode system prompt to Pi agent turns;
- exposes bundled Octocode skills to Pi through package metadata and
  `resources_discover`;
- bridges Pi `write`/`edit` tool calls to bundled `octocode-awareness` file
  locks when the awareness script is available;
- provides setup/status/update commands;
- keeps MCP optional, because Pi core is intentionally lean and does not ship
  native MCP as a baseline requirement.

The build excludes secret env files such as `skills/**/.env` and generated
Python caches, but keeps `.env.example` files.

## Commands

| Command | Purpose |
|---|---|
| `/octocode-status` | Show bundled prompt and skills status. |
| `/octocode-setup` | Write the managed Octocode prompt block to project `.pi/APPEND_SYSTEM.md`. |
| `/octocode-setup --global` | Write the managed Octocode prompt block to `~/.pi/agent/APPEND_SYSTEM.md`. |
| `/octocode-mcp-install [args]` | Confirm, then run `npx octocode install ...` for MCP-native clients. |
| `/octocode-skills-update` | Confirm, then run `pi update npm:@octocodeai/pi-extension` and reload Pi resources. |

The package already loads the bundled skills. Use `/octocode-setup` when you
also want a durable `APPEND_SYSTEM.md` file on disk.

`/octocode-status` reports whether the Pi-native awareness file-lock bridge can
find the bundled `octocode-awareness` script. If it is missing, Pi continues
normally and the awareness skill can still be used manually.

## Skill Cookbook

Use the smallest skill that matches the job:

| Skill | Use for |
|---|---|
| `octocode` | Quick Octocode transport guidance and lookup recipes. |
| `octocode-research` | Default code research, implementation, review, refactoring, debugging, PR/history, binary, AST, LSP, and architecture work. Covers everything `octocode-engineer` used to handle. |
| `octocode-awareness` | Memory, file locks, handoff state, verification records, and durable learning. |
| `octocode-brainstorming` | Evidence-grounded idea exploration and prior-art validation. |
| `octocode-rfc-generator` | RFCs, migration plans, architecture proposals, and risky pre-implementation plans. |
| `octocode-roast` | Explicit hard review and evidence-backed critique. |
| `octocode-skills` | Search, compare, lint, create, and update skills. |
| `octocode-stats` | Octocode usage stats and reports. |

In Pi, call skills with `/skill:<name>` when you want to force a mode, or let
the package make them available for normal Pi skill discovery.

## Operating Model

- Prefer Pi skills plus the Octocode CLI for Pi-native work.
- Prefer MCP tools for MCP-native hosts.
- Treat MCP, subagents, plan mode, permission gates, and sandboxing as optional
  Pi extensions or external wrappers, not assumptions about Pi core.
- Read live schemas/help before raw tool calls.
- Treat search results as leads; exact reads, tests, schemas, and runtime output
  are proof.
- Keep active context small; write durable handoffs and research receipts to
  files before compaction or delegation.
- Use `octocode-awareness` when work is long-running, concurrent, dirty, or
  worth remembering.
- Route risky or cross-package work through `octocode-rfc-generator` before
  implementation.

## Pi Claim Check

Research against upstream Pi docs and source supports these claims:

- Pi is a minimal terminal coding harness designed to stay small at the core and
  be extended through skills, TypeScript extensions, prompt templates, themes,
  and Pi packages.
- Pi loads context files, `APPEND_SYSTEM.md`, skills, slash commands,
  non-interactive print mode, custom models, and compaction.
- Pi does not include built-in MCP, subagents, permission popups, plan mode,
  to-dos, background bash, or a default permission sandbox. Those are extension
  or environment choices.
- Pi has extension examples for subagents, plan mode, permission gates,
  protected paths, tool routing, and sandboxing. The Octocode harness should
  recommend them as optional upgrades, not baseline requirements.

## Use `npx octocode` First

For Pi, prefer the Octocode CLI. In normal Pi workflows, the bundled skills plus
`npx octocode` are the recommended path and are good enough for code research,
navigation, GitHub/package lookup, and LSP-backed checks. You do not need MCP
just to use Octocode well from Pi.

```bash
npx octocode search "<term>" <path-or-owner/repo>
npx octocode search <path> --tree
npx octocode auth status --json
```

MCP is optional. Add it only when you specifically want Pi to call Octocode
through MCP tools, or when you are configuring another MCP-native client.

### Optional: Octocode MCP inside Pi

Install a Pi MCP adapter, then restart Pi:

```bash
pi install npm:pi-mcp-adapter
```

Add Octocode to a project-local `.mcp.json` file:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "@octocodeai/mcp@latest"]
    }
  }
}
```

Use `~/.config/mcp/mcp.json` instead when you want the same server available in
all projects. Add `env` values such as `GITHUB_TOKEN`, `GH_TOKEN`, or
`ENABLE_CLONE=true` only when that project needs them.

The adapter reads standard MCP config files and keeps servers lazy by default,
so the Octocode MCP server is not started until the agent actually calls it.

### Configure other MCP clients

For Cursor, Claude Code, VS Code, and other MCP-native hosts, use the Octocode
installer:

```bash
/octocode-mcp-install --ide cursor
```

That command asks before running `npx octocode install ...` because it may write
external client configuration. You can also run the CLI directly:

```bash
npx octocode install --ide cursor
```

## Development

Build the package with the repo's Yarn release:

```bash
yarn workspace @octocodeai/pi-extension build
```

Verify just this package:

```bash
yarn workspace @octocodeai/pi-extension verify
```

The build refreshes the ignored package-local `skills/` mirror from the root
`skills/` directory, then copies that mirror into `dist/skills/`. The npm
package ships both `skills/` and `dist/skills/` so Pi can load skills from the
package root and the extension can use its generated runtime assets.

## References

- [Octocode APPEND_SYSTEM.md](https://github.com/bgauryy/octocode/blob/main/packages/octocode-pi-extension/docs/PI/APPEND_SYSTEM.md)
- [Octocode Skills](https://github.com/bgauryy/octocode/blob/main/skills/README.md)
- [Pi packages](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/packages.md)
- [Pi extensions](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/extensions.md)
- [Pi skills](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/skills.md)
- [Pi MCP adapter](https://github.com/nicobailon/pi-mcp-adapter)
- [Octocode MCP](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md)
