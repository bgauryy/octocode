# MCP and CLI reference

Run the package from the workspace where you want to save captures. The executable starts a stdio MCP server by default. Use `/cli` for typed commands or `/raw` for the underlying command-line syntax. The examples use the installed executable. With npm, replace `octocode-chrome-devtools` with `npx -y @octocodeai/octocode-chrome-devtools`. [Requirements and MCP setup](../README.md#quick-start)

## MCP flags

| Argument | Effect |
|---|---|
| `--mcp --preset slim` | Expose five tools for launching Chrome, running plans, searching results, reading guidance, and cleanup. |
| `--mcp --preset research` | Expose the 12 default tools, including `protocol`, `cdp`, and `flow`. |
| `--mcp --preset full` | Expose all 19 MCP tools. |
| `--tools <names>` | Add individual tools or groups to the selected preset. Separate names with commas. |

The groups are `capture` (`check`), `data` (`session`, `cookies`, `script`), and `devtools` (`dev-check`, `helper`, `proxy`). For example, `--mcp --preset research --tools capture` keeps the default tools and adds capture recipes. See the [complete tool list](../README.md#tools-and-presets).

## Choose the input

| You need | Use |
|---|---|
| A command, plan step, or recipe’s fields | `schema` with `command` and optionally `operation` or `recipe`. The MCP schema lists commands in its active preset. |
| A sequence of page actions and observations | `run` with a `plan` and optional `connection`. Use a `snapshot` or `screenshot` step for captures in every MCP preset. [Plan examples](browser-execution.md) |
| A domain or method available in your Chrome | `protocol` to inspect the live schema, then `cdp` with the method and its parameters. [CDP routing](cdp-protocol.md) |
| A particular tab | Set `connection.target` to the ID from `open` or `targets`. For a named tab group, use `group` and `groupIndex` in the connection. [Tab selection](tabs-and-policy.md) |
| A reusable plan | Use `flow` to list, inspect, save, or run a plan. [Flows](flows.md) |

```sh
octocode-chrome-devtools /cli schema --command run --operation extract --json
octocode-chrome-devtools /cli schema --command check --recipe page-snapshot --json
octocode-chrome-devtools /cli skill --topic browser-execution --json
```

For `/cli run --input plan.json --json`, put the **whole tool input** in the file. This plan reads the current page’s main headings. Start a browser with `open` first. Add a connection when you need to select a particular session or tab.

```json
{"plan":{"steps":[{"op":"extract","selector":"h1","fields":["text"]}]}}
```

Use `--input -` to read the same JSON from stdin. For `/raw run --plan`, the file contains only the inner plan. To validate that plan without launching Chrome, use `/raw run --plan <file> --dry-run`.

## Read the result

MCP places the successful reply in `structuredContent` and its text block. The typed CLI exposes `structuredContent` with `--json`. Read `data` or `snapshot` for inline results, `files` for captures, and `evidence` for the manifest. Search saved rows with `query` and read a known range with `artifact`. Use strings for integer filters outside JavaScript’s safe integer range. [Evidence files](evidence-files.md)

Each continuation contains `{tool, query}`. In MCP call that tool with `query` as its arguments. In the CLI, save the query to a file and pass it with `--input` from the same workspace. Follow the returned continuations to read remaining rows, artifacts, or logs.

An MCP failure sets `isError`. The text block gives the cause in `summary` and a `next` call when more evidence is available. A cancelled action may already have changed the page. Inspect the result before retrying. [Recovery](recovery.md#bounded-retry)
