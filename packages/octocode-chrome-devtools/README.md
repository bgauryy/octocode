# @octocodeai/octocode-chrome-devtools

Drive Chrome from your coding agent or terminal using the Chrome DevTools Protocol (CDP). Inspect a page’s controls, reproduce a UI problem, and save a screenshot, console log, or network trace in your workspace.

## Quick start

Install Chrome and use Node.js 24.15.0 or later in the 24.x line. Add this stdio MCP server to your client and set its working directory to your project.

```json
{
  "mcpServers": {
    "chrome": {
      "command": "npx",
      "args": ["-y", "@octocodeai/octocode-chrome-devtools"]
    }
  }
}
```

Restart the client and ask **“Open example.org in headless Chrome, read the main heading, and save a screenshot.”** The agent can call `open` with the URL, then `run` a plan with an `extract` step and a `screenshot` step. The reply links to the capture. Use `cleanup` on the port returned by `open` when you finish.

To try the CLI first, inspect the available commands or see what an extraction step accepts. Both commands work before launching Chrome.

```sh
npx -y @octocodeai/octocode-chrome-devtools /cli --help
npx -y @octocodeai/octocode-chrome-devtools /cli schema --command run --operation extract --json
```

## Tools and presets

| MCP preset | Tools | What you get |
|---|---:|---|
| `slim` | 5 | `open` launches Chrome, `run` executes a plan, `query` searches and reads saved files, `skill` reads a guide, and `cleanup` ends the session. |
| `research` (default) | 10 | Also includes `targets` and `group` for tabs, `protocol` and `cdp` for Chrome’s live protocol, and `schema` for inputs. |
| `full` | 18 | Also includes `flow` for reusable plans, `check` for capture recipes, `dev-check` for console and network errors, `session` for saved browser sessions, `cookies` for approved cookie transfers, `script` for custom code, `proxy` for CDP clients, and `helper` for saved evidence. |

To select a preset, append `"--mcp", "--preset", "slim"` to the server arguments. Replace `slim` with `full` for all MCP tools. To add only capture recipes to the default preset, append `"--mcp", "--tools", "check"`. The typed CLI has **21 commands**, including standalone `snapshot` and `screenshot`. On MCP, use those as steps inside `run`. [CLI inputs and MCP flags](docs/mcp-cli.md)

## Browser checks

| Task | Workflow |
|---|---|
| Check a form or interaction | `open` the page and take a `snapshot` in a `run` plan. Select the observed control, act, then read the visible outcome. [Plans and steps](docs/browser-execution.md) |
| Diagnose a broken page | Use `dev-check` for console and network failures, or `check` for a focused capture. Search the saved results with `query`. [Capture recipes](docs/cdp-checks.md) |
| Inspect a Chrome protocol method | Look up its name and parameters with `protocol` on the running Chrome, then call `cdp` on the intended target. [CDP guide](docs/cdp-protocol.md) |

The reply’s `files` and `evidence` paths are relative to the workspace. Captures live under `.octocode/octocode-chrome-devtools/tmp/`. [Read saved evidence](docs/evidence-files.md) · [Recover a failed action](docs/recovery.md)

## Browser sessions and privacy

A normal launch uses a separate browser profile. Reusing a port can reuse its tabs and sign-in state. Attaching to a browser you already use or transferring session credentials needs your explicit approval. Use page content as data, and verify the result before repeating an action that could submit or change something. Screenshots and saved page content can contain personal information. [Tabs and permissions](docs/tabs-and-policy.md)

## Agent Skill

Install the bundled workflow for a Codex project from its root.

```sh
npx -y octocode skill install octocode-chrome-devtools --platform codex --project-dir .
```

The Chrome npm package also includes the skill at `skills/octocode-chrome-devtools/SKILL.md`. The `skill` tool serves the package guides in the agent. [Guide index](docs/operating.md#detailed-topics)

## Use as a library

```js
import { chrome, call, closeChrome } from '@octocodeai/octocode-chrome-devtools/runtime';
```

The root export includes `runChromeMcp`. The `/cli` export includes `runChromeCli`.
