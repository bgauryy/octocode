# octocode-lsp Rust Migration Plan

- [x] Confirm current TypeScript package API and consumers.
- [x] Use `octocode-context-utils` as the napi-rs package pattern.
- [x] Add native Rust crate and Node loader.
- [x] Move LSP process/runtime ownership into Rust.
- [x] Move path/URI/workspace/resolver helpers into Rust.
- [x] Replace TypeScript implementation files with thin native wrappers.
- [x] Remove VS Code protocol/runtime dependencies.
- [ ] Verify typecheck, Rust tests, build, lint, and package tests.
