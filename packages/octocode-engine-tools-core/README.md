# Octocode engine tools core

Rust implementation of Octocode's tool runtime and native `octo` CLI, developed
beside the frozen Node reference. Node is retained only for the thin MCP
interface calling the optional NAPI addon.

Implementation is in progress. Native configuration, request lifecycle, security,
file reads and MCP addon execution are implemented. The first localFetch suites
match the frozen reference in 18 CLI cases and 16 MCP cases. Expanded feature
coverage and performance gates remain open; the other tools are unavailable.
The binary never falls back to Node.

```sh
cargo build --manifest-path packages/octocode-engine-tools-core/Cargo.toml --bins --no-default-features
cargo run --manifest-path packages/octocode-engine-tools-core/Cargo.toml --bin octo -- read src/index.ts --lines 20:60
cargo build --manifest-path packages/octocode-engine-tools-core/Cargo.toml --lib --features napi-addon
cargo test --manifest-path packages/octocode-engine-tools-core/Cargo.toml --no-default-features
```

The native default feature set excludes NAPI. Tool contracts are generated from
the canonical public core; config, policy, execution and rendering are Rust.
Existing engine algorithms remain in their owning portable Rust library.

The candidate MCP entry is `packages/octocode-mcp/src/native/index.mjs`; its
`OCTOCODE_NATIVE_BINDING` setting identifies the built addon artifact. Platform
packaging and production entry-point promotion are not complete. Build native
binaries and the addon separately: NAPI symbols belong only in the addon.
