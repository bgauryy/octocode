# Octocode tools-core

`@octocodeai/octocode-tools-core` is the shared execution layer behind the
Octocode CLI and MCP server. It runs GitHub, package, local search, AST, rewrite,
file-fetch, and LSP tools with common security, pagination, response shaping,
credentials, and session behavior.

This package does not own public tool descriptions or schemas. Those contracts
come from `@octocodeai/octocode-core`; native primitives and LSP clients come
from `@octocodeai/octocode-engine`.

## Public entries

- `@octocodeai/octocode-tools-core` — server-oriented runtime exports.
- `@octocodeai/octocode-tools-core/direct` — direct tool catalog and execution.
- `@octocodeai/octocode-tools-core/schema` and `/zod` — runtime schema adapters.
- `/credentials`, `/config`, `/platform`, `/session`, `/paths`, and `/fs-utils`
  — focused shared services.

Use the Octocode CLI or MCP server for normal agent-facing operation. Direct
consumers must pass the same bulk query envelopes used by those interfaces and
must preserve structured partial-result continuations.

## Development

From the repository root:

```bash
yarn workspace @octocodeai/octocode-tools-core build
yarn workspace @octocodeai/octocode-tools-core test
yarn workspace @octocodeai/octocode-tools-core lint
yarn workspace @octocodeai/octocode-tools-core typecheck
```

See [architecture](ARCHITECTURE.md), the repository
[tool reference](../../docs/OCTOCODE_TOOLS.md), and the
[security model](../../docs/SECURITY.md).

## License

MIT
