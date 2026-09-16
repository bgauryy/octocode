# `octocode-engine-core` Architecture

`octocode-engine-core` is the canonical Rust owner of reusable Octocode engine primitives. It is an internal crate, not a published JavaScript package.

## Allowed dependency direction

```text
octocode-engine ──┐
                  ├──> octocode-engine-core
octocode-native ──┘
```

The core crate must not depend on either consumer. `octocode-engine` owns N-API bindings, JavaScript loaders, and Node host orchestration. `octocode-native` owns tool contracts, policy, providers, rendering, runtime composition, and the CLI.

## Ownership rules

- Shared Rust algorithms, domain models, errors, security primitives, and LSP lifecycle policy live here exactly once.
- Transport DTO conversion belongs at the consumer boundary; it must not fork domain behavior.
- Default builds are portable and contain no required Node runtime dependency.
- `napi-addon` is optional and exists only where one canonical Rust type must participate in the existing N-API surface.
- Consumers pass resolved policy/configuration into core operations; core does not import runtime, provider, CLI, or MCP packages.
- Move implementations rather than copying them. The outer crates contain only boundary adapters or runtime composition, not compatibility copies of migrated modules.

## Verification

Run `yarn workspace @octocodeai/octocode-engine-core verify`, then the engine addon/FFI tests and native runtime tests. Dependency checks must prove that this crate has no reverse edge and that `octocode-native` no longer imports `octocode-engine` after migration.
