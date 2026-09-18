# Octocode engine

`@octocodeai/octocode-engine` provides Octocode's reusable Rust research primitives: bounded lexical and filesystem search, structural search and rewrite, signatures, graph facts and algorithms, indexed search, LSP transport and lifecycle, minification, text utilities, and secret detection.

Prebuilt N-API addons are delivered through platform-specific optional packages for macOS, Linux, and Windows. Node consumers depend on the root package and its minimal loader selects the matching addon. Rust consumers such as `octocode-native` link the crate directly without N-API.

## Requirements

- Node.js 24.15.x for the npm package
- A supported platform package for N-API operations

Published packages do not require a Rust toolchain.

## Public surface

The root export exposes primitive native functions. The package has no TypeScript LSP, security, or tool-execution layer and no subpath runtime APIs.

Application-facing tool contracts, policy composition, providers, pagination, and responses belong to `@octocodeai/octocode-native`.

## Development

From the repository root:

```bash
yarn workspace @octocodeai/octocode-engine build:dev
yarn workspace @octocodeai/octocode-engine typecheck
yarn workspace @octocodeai/octocode-engine test
yarn workspace @octocodeai/octocode-engine test:rust
yarn workspace @octocodeai/octocode-engine platforms:check
```

See [engine architecture](ARCHITECTURE.md), [LSP lifecycle](docs/LSP_SERVER_LIFECYCLE.md), and [supported languages and features](docs/SUPPORTED_LANGUAGES_AND_FEATURES.md).

## License

MIT
