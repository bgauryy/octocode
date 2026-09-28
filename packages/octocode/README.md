# Octocode CLI

`octocode` is the command-line interface for Octocode research tools, local
workspace analysis, GitHub research, package discovery, skill management, and
MCP client setup.

The CLI is a presentation layer. Tool contracts are authored in
`@octocodeai/octocode-core` and consumed through `@octocodeai/config`;
execution comes from `@octocodeai/octocode-native`, including native search and
LSP support from its internal engine crate.

## Requirements

- Node.js 24.15.x

## Run

```bash
npx octocode --help
```

Inspect the available tools and the exact schema before an unfamiliar call:

```bash
npx octocode scheme --compact
npx octocode scheme localSearch --view query --compact
```

Common management commands:

- `octocode install` configures supported MCP clients.
- `octocode auth` shows GitHub authentication; `auth login` / `auth logout` manage it.
- `octocode config` shows config file paths and set key names (never values).
- `octocode skill` manages bundled Agent Skills (`list`, `install`, `check`,
  `info`, `remove`); `skill check --fix` repairs stale or broken installs.

Research runs through one command per tool: `octocode <toolName> '<json>'`
(e.g. `octocode localSearch '{…}'`). Results use structured exit codes and
expose executable `next` calls whenever more data is reachable.

## Development

From the repository root:

```bash
yarn workspace octocode build
yarn workspace octocode test
yarn workspace octocode lint
yarn workspace octocode typecheck
node packages/octocode/out/octocode.js --help
```

See [CLI architecture](ARCHITECTURE.md), [CLI reference](docs/OCTOCODE_CLI.md),
and the repository [tool reference](../../docs/OCTOCODE_TOOLS.md).

## License

MIT
