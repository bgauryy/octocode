# octocode-mcp-cli architecture

`octocode-mcp-cli` is a private workspace library. It builds a CLI from a connected MCP `Client`: `initialize` instructions, `tools/list`, and `tools/call`. The library does not import the `octocode` launcher or `octocode-mcp`. [examples/octocode-cli.ts](examples/octocode-cli.ts) spawns the built `octocode-mcp` server. The `octocode` launcher stays a separate program: `octocode <tool> '<json>'`.

## Ownership

- `CliSpec` is the only model shared by help, the runner, codegen, and both MCP directions.
- Zod commands validate with Zod. MCP commands keep the original JSON Schema and validate with Ajv.
- Help reads the normalized flag list and the JSON Schema behind each partial flag. Zod commands and MCP tools share that screen. The example value includes the required fields.
- `emitTypeScript` writes one generated module. Callers keep extra commands in a second module.
- Server instructions map to MCP `initialize` instructions. They are not copied onto each command.
- The help contract is in [README.md](README.md).

## Invariants

- A property outside the typed subset stays on the command as one partial JSON flag.
- A default makes the CLI flag optional.
- An imported tool exports the same `inputSchema` object that arrived from MCP.
- An authored tool exports `z.toJSONSchema` of its Zod schema.
- Generated source stores server text in `JSON.stringify` literals.
- The command word is the MCP tool name when it is one argv word of letters, digits, `.`, `_`, and `-`. `help` and `version` get a suffixed token. `tools/call` and MCP export use the original name.
- Command help shows the description, the display title, the usage line, the flags, the output schema, and the annotation hints the server sent. Fidelity notes stay off that screen. A result object prints as `key: value` lines or a table. The screen keeps every leaf and every content field. `--json` prints that result. It keeps content together with structured content when the server sent both.
- A tool absent from `tools/list` is absent from the CLI. `octocode-mcp` omits the CLI-only tools `ghCloneRepo` and `astRewrite`.
- `runCli` returns a status code. It does not exit the process.
