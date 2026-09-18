# Octocode engine compatibility package

`@octocodeai/octocode-engine` is a JavaScript-only compatibility package. It
re-exports `@octocodeai/octocode-native/engine` for existing CommonJS, ESM, and
TypeScript consumers.

The reusable Rust crate, N-API bindings, loaders, tests, documentation, and all
six platform artifacts are owned by `packages/octocode-native`. This package
must not regain Rust source, platform packages, optional native dependencies, or
an independent build/release pipeline.
