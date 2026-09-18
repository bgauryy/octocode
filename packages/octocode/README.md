# Octocode CLI

`octocode` is the command-line interface for Octocode research tools, local
workspace analysis, GitHub research, package discovery, skill management, and
MCP client setup.

The CLI is a presentation layer. Tool contracts come from
`@octocodeai/octocode-core`, execution comes from
`@octocodeai/octocode-tools-core`, and native search and LSP support come from
`@octocodeai/octocode-engine`.

## Requirements

- Node.js 24.15.x

## Run

```bash
npx octocode --help
```

Inspect the available tools and the exact schema before an unfamiliar call:

```bash
npx octocode tools --json --compact
npx octocode tools localSearch --scheme --json --compact
```

Common management commands:

- `octocode install` configures supported MCP clients.
- `octocode auth` manages GitHub authentication.
- `octocode status` reports authentication, cache, and client health.
- `octocode skill` manages bundled Agent Skills.
- `octocode lsp-server` manages local language servers.

Research runs through `octocode tools <name>`. Results use structured exit
codes and expose executable `next` calls whenever more data is reachable.

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
