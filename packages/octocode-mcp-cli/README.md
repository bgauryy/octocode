# octocode-mcp-cli

Private library that builds a CLI from an MCP server. Connect with the MCP client, read `initialize` and `tools/list`, and run each tool as a command. A command call sends `tools/call`.

This package does not execute Octocode tools and does not replace the `octocode` launcher.

## MCP server to CLI

```ts
import { cliFromMcp, connectStdio, runCli } from 'octocode-mcp-cli';

const client = await connectStdio({ command: 'node', args: ['server.js'] });
try {
  const cli = await cliFromMcp(client);
  process.exitCode = await runCli(cli, process.argv.slice(2));
} finally {
  await client.close();
}
```

`connectStdio` and `connectStreamableHttp` perform the MCP handshake through `@modelcontextprotocol/sdk`. `cliFromMcp` takes that connected `Client`. Server instructions are the default output and the root `--help`. Each tool name is a command. The tool description is that command's `--help` lead. The tool `inputSchema` is the usage line and the FLAGS section. `runCli` sends `tools/call` with the flag values and prints the tool result.

`connectStreamableHttp(url)` is the same path for a Streamable HTTP server.

## Build a CLI, add a command, expose it

[examples/mcp-to-cli.ts](examples/mcp-to-cli.ts) connects to the official everything server, adds `cli-doctor`, and can serve that command list as MCP.

```bash
node examples/mcp-to-cli.ts
node examples/mcp-to-cli.ts echo --message "hello from mcp"
node examples/mcp-to-cli.ts cli-doctor
node examples/mcp-to-cli.ts --expose
```

`--expose` serves the imported tools plus `cli-doctor` on stdio. The upstream server stays a child process. This process speaks MCP on its own stdin and stdout.

```ts
const imported = await cliFromMcp(client);
let spec = imported;
const doctor = defineCommand({
  name: 'cli-doctor',
  title: 'CLI doctor',
  description: 'List the commands on this CLI.',
  annotations: { readOnlyHint: true },
  schema: z.object({}),
  run: () => spec.commands.map(command => command.name),
});
spec = withCommands(imported, [doctor]);

process.exitCode = await runCli(spec, process.argv.slice(2));

const server = new McpServer({ name: spec.name, version: spec.version }, mcpServerOptions(spec));
registerOn(server, spec);
```

`withCommands` appends commands. `help` and `version` stay reserved. `defineCli` rejects a second command with the same name.

## Octocode tools

`packages/octocode` is the terminal launcher. A call is `octocode <tool> '<json>'`. That process does not speak MCP. [examples/octocode-cli.ts](examples/octocode-cli.ts) spawns the built `packages/octocode-mcp` server and turns `tools/list` into this CLI.

```bash
node examples/octocode-cli.ts
node examples/octocode-cli.ts localSearch --help
node examples/octocode-cli.ts localSearch --queries '[{"matchString":"value","path":"packages/octocode-mcp-cli/src/help.ts"}]'
```

Build `octocode-mcp` before the example. The script reads `packages/octocode-mcp/dist/index.js` and exits 2 when that file is missing. A tool absent from `tools/list` is absent here. The server omits the CLI-only tools `ghCloneRepo` and `astRewrite`. This library does not import `octocode` or `octocode-mcp`.

## Reference servers

[examples/everything-cli.ts](examples/everything-cli.ts) and [examples/filesystem-cli.ts](examples/filesystem-cli.ts) connect to the official everything and filesystem servers. Build this package first, then run the examples from this directory.

```bash
node examples/everything-cli.ts echo --message "hello from mcp"
node examples/everything-cli.ts get-sum --a 2 --b 3

node examples/filesystem-cli.ts /allowed/dir list_allowed_directories
node examples/filesystem-cli.ts /allowed/dir list_directory --path /allowed/dir
node examples/filesystem-cli.ts /allowed/dir read_text_file --path /allowed/dir/note.txt
```

`filesystem-cli.ts` requires the allowed directory as its first argument. The server can access only that directory. A filesystem tool result prints as `key: value` lines or a table when the server returns structured content. `--json` prints the full result object.

`yarn workspace octocode-mcp-cli test:servers` runs the same two servers.

## Add a command

```ts
import { defineCli, defineCommand, runCli, withCommands } from 'octocode-mcp-cli';
import { z } from 'zod';

const search = defineCommand({
  name: 'search',
  description: 'Search issues by text.',
  schema: z.object({
    query: z.string().describe('Search text'),
    limit: z.number().int().optional().describe('Maximum rows'),
  }),
  run: async input => input,
});

const cli = defineCli({
  name: 'issues',
  instructions: 'Search before you write. Use --json when the caller is an agent.',
  commands: [search],
});

process.exitCode = await runCli(cli, process.argv.slice(2));
```

`withCommands(cli, [extra])` appends commands. `help` and `version` stay reserved. `defineCli` rejects a second command with the same name.

## Help contract

| Invocation | Stdout | Exit |
|---|---|---|
| `<bin>` | Instructions, usage, and the command index | 0 |
| `<bin> --help`, `<bin> -h`, `<bin> help` | The same text, plus global flags, one example, and a learn-more line | 0 |
| `<bin> <command> --help` | Description, display title, usage, flags, output schema, hints, and a validated example when available | 0 |
| `<bin> <command> --help --json` | `{ name, description, title, usage, flags, inputSchema, outputSchema, annotations }` | 0 |
| `<bin> <command> …` missing a required flag | The missing flag name, then that command's help, on stdout | 2 |
| `<bin> <command> …` valid | `key: value` lines or a table for an object, then any content blocks in full. `--json` prints the full result | 0 or 1 |

Help writes to stdout. A known command exits 0; an unknown command exits 2. Tool results write to stdout. Errors write to stderr. Exit 0 is success, exit 2 is usage, and exit 1 is a tool error without an explicit exit status. `runCli` returns the code. It does not call `process.exit`.

Global flags reserved from schemas: `--help`, `-h`, `--version`, `--json`, `--no-input`, and `--no-color`. A schema property with one of those names gets a suffixed flag. The property name stays the object key.

The usage line lists required flags first, then optional flags in brackets. A string, integer, number, enum, boolean, or primitive array becomes a typed flag. A nested object, `$ref`, combiner, or array of objects becomes one `--name <json>` flag. The FLAGS section lists each field inside that value with its type, required mark, and default. The example quotes an enum value that contains a space, and it quotes a JSON value that includes the required fields. Ajv still validates the original schema.

Screen sections are `USAGE`, `COMMANDS`, `FLAGS`, `OUTPUT`, `HINTS`, `EXAMPLES`, and `LEARN MORE`. Bare output omits the global flags. The command index uses the display title and the first sentence. Command help keeps the full description. `OUTPUT` and `HINTS` appear only when the command has an output schema or an annotation hint. Fidelity notes stay on the command object and stay off this screen. Zod commands and MCP tools share this screen.

`cliFromMcp` pages `tools/list` until `nextCursor` is absent. The command word is the tool name, including dots. `title`, then `annotations.title`, is the display label. The input schema supplies the flags. The output schema is the OUTPUT section. Annotation hints the server sent appear under HINTS. A record prints as `key: value`. A list of records prints as a table. A nested value prints under its key. The screen prints every leaf in full, including trailing spaces in a table cell. Text content prints in full. Every other content block prints as JSON with its fields, including image data, audio data, resource text, and resource blob. When the server sends structured content and content, the human screen prints both. `--json` keeps both on successful calls, including `_meta`. A tool error writes its content to stderr and exits 1. Error structured content and metadata are not preserved by the current error path.

`emitTypeScript(spec, filePath)` writes the imported commands to one module. Descriptions and instructions are `JSON.stringify` literals. Add further commands in a separate file.

`cliToMcp(spec)` returns protocol tool definitions for a CLI you already have. `registerOn(server, spec)` registers them. `mcpServerOptions(spec)` is `{ instructions }` for the `McpServer` constructor.

## Current fidelity limits

Successful MCP calls retain content blocks, structured content, and metadata in CLI JSON output.
Tool errors become exceptions. Their content reaches stderr, but structured error details and metadata do not survive.
`registerOn` exports CLI results as text, with structured content when available.
It does not preserve original multimodal content blocks or metadata when re-exporting an imported MCP result.
Use the upstream MCP client directly when those round-trip fields matter.

## Ownership

See [ARCHITECTURE.md](ARCHITECTURE.md).

### Help and execution behavior

Command examples are validated against the complete input schema, including conditional requirements. A schema that cannot produce a valid sample has no generated example; it still exposes its full flags and JSON schema. Authored JSON Schema `examples` take priority. Array flags accept repeated values or a JSON array, including an empty array. `undefined` handler results emit no output, so streaming handlers do not append an extra frame. Errors with an integer `exitCode` between 1 and 255 preserve that status. Standard JSON Schema formats are validated by `ajv-formats`.
