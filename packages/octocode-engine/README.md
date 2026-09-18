# Octocode engine

`@octocodeai/octocode-engine` is Octocode's native research engine and
TypeScript LSP/security orchestration layer. Rust provides bounded filesystem
search, minification, structural analysis, signatures, text utilities, and
secret detection. TypeScript owns language-server lifecycle, semantic
navigation, and security adapters.

Prebuilt native addons are delivered through platform-specific optional
packages for macOS, Linux, and Windows. Consumers normally depend on the root
package and let the loader select the matching addon.

## Requirements

- Node.js 24.15.x
- A supported platform package for native operations

No Rust toolchain is required when using a published prebuilt package.

## Public surface

The root export exposes native research primitives. Focused subpath exports
provide LSP and security services, including content sanitization, path and
command validation, masking, and the security registry.

Application-facing research behavior belongs in
`@octocodeai/octocode-tools-core`; this package stays at the native and
language-service boundary.

## Development

From the repository root:

```bash
yarn workspace @octocodeai/octocode-engine build
yarn workspace @octocodeai/octocode-engine test
yarn workspace @octocodeai/octocode-engine test:rust
yarn workspace @octocodeai/octocode-engine platforms:check
```

See [engine architecture](ARCHITECTURE.md),
[LSP lifecycle](docs/LSP_SERVER_LIFECYCLE.md), and
[supported languages and features](docs/SUPPORTED_LANGUAGES_AND_FEATURES.md).

## License

MIT
