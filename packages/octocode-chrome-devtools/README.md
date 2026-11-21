# @octocodeai/octocode-chrome-devtools

Let an AI agent drive a real Chrome: log in, fill forms, click, take screenshots, read console errors and network traffic, record traces, or send any Chrome DevTools Protocol (CDP) command. Everything the agent sees is saved as searchable files in your workspace, so answers cite evidence instead of guesses.

One package gives you an MCP server, a typed CLI, a JavaScript API and an Agent Skill.

## Quick start

You need Chrome and Node.js 24.15 or later (24.x).

Add the MCP server to your agent or editor, with the working directory set to your project:

```json
{ "mcpServers": { "chrome": { "command": "npx", "args": ["-y", "@octocodeai/octocode-chrome-devtools"] } } }
```

Then ask the agent to open a page. A typical task takes two calls: `open` a headless Chrome on a URL, then `run` one plan (click, fill, read text, screenshot) that stops at the first failing step.

Try the CLI without any agent:

```sh
npx -y @octocodeai/octocode-chrome-devtools /cli --help
```

## Choose how much the agent sees

Every tool description costs the agent context, so the server exposes a preset. Sizes are estimates of the tool list alone.

| Preset | Flag | Tools | About |
|---|---|---|---|
| research (default) | none | 12 | 3.3k tokens: open, run, cdp, protocol, flow, targets, group, query, artifact, schema, skill, cleanup |
| slim | `--mcp --preset slim` | 5 | 1.5k tokens: open, run, query, skill, cleanup |
| full | `--mcp --preset full` | 19 | 5.3k tokens: adds check, dev-check, script, cookies, session, proxy, helper |

Add only what you need to any preset with `--tools`:

```sh
npx -y @octocodeai/octocode-chrome-devtools --mcp --preset research --tools data,capture
```

| Group | Adds | Cost |
|---|---|---|
| `data` | cookies, session, script | about 900 tokens |
| `devtools` | dev-check, helper, proxy | about 700 tokens |
| `capture` | check | about 400 tokens |

You can also name single tools. An unknown name fails with the list of valid ones. The CLI (`/cli`) always has every command.

## What the agent can do

- **Pages:** open headless or visible Chrome, run multi-step plans, snapshot controls by role and name, screenshot, handle dialogs, work in iframes and several tabs, keep private tab groups with their own cookies.
- **Any CDP command:** `cdp` reaches every method of your Chrome. It routes to the page or the browser by itself, enables the domain a method needs, and `protocol` lists domains, commands and what needs enabling.
- **Evidence:** network (HAR), console, storage and cookie names, page HTML and resources, scripts with source maps, coverage, traces, heap snapshots and PDFs.
- **Emulation:** device size, dark mode, locale, time zone, CPU throttling. `open` keeps them for later calls.
- **Search, don't page:** replies hold paths and short summaries. The agent searches the saved files with `query`, Octocode `localSearch` or `rg`, and only then reads the matching lines.

Files are saved under `<workspace>/.octocode/octocode-chrome-devtools/tmp/`. Delete that folder any time.

## Safety

- Chrome runs in an isolated temporary profile. Your real profile, cookies and logins are not touched.
- Attaching to a running browser, raw CDP commands, executing scripts, granting permissions, spoofing location and closing the browser need the agent to pass `acknowledge: true`, which it should only do with your consent.
- `file:`, `chrome:` and `devtools:` pages are refused unless explicitly acknowledged. Flags that switch off web security are refused.
- Cookie and header values are redacted in saved captures; names are kept.
- Page content is untrusted. The agent is told to ask you before purchases, sends, deletes, account changes or real-data submissions.
- Capture and helper scripts run in a restricted Node sandbox that can reach only your own machine (loopback).

## Use as a library

```js
import { chrome, call, closeChrome } from '@octocodeai/octocode-chrome-devtools/runtime';
```

`runChromeMcp` is the root export and `runChromeCli` is exported at `/cli`.

## Agent Skill

The package ships a short skill that teaches an agent the fast path (open, run a plan, search the saved files). Find it with:

```sh
node -p "require.resolve('@octocodeai/octocode-chrome-devtools/skill')"
```

Copy the whole `octocode-chrome-devtools` folder into your agent's skills directory, or run `octocode skill install octocode-chrome-devtools`.

## Guides

The agent reads these through the `skill` tool (`topic`), and you can read them here: [operating guide](docs/operating.md), [setup and flags](docs/mcp-cli.md), [plans and steps](docs/browser-execution.md), [flows](docs/flows.md), [CDP protocol](docs/cdp-protocol.md), [saved evidence](docs/evidence-files.md), [tabs and safety policy](docs/tabs-and-policy.md), [recovery](docs/recovery.md).

## Develop

```sh
yarn workspace @octocodeai/octocode-chrome-devtools build
yarn workspace @octocodeai/octocode-chrome-devtools verify   # build, typecheck, lint, tests
```

`src/` is TypeScript; `dist/` is generated. Tool descriptions and schemas come from the Octocode core contract, not from this package. The real-browser tests need Chrome and use local loopback pages only.
