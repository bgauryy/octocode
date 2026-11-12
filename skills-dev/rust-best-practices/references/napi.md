# napi-rs boundaries

Load only for a project that actually uses napi-rs. Inspect its `Cargo.lock`, Cargo features, `napi`/`napi-derive` versions, CLI version, Node-API baseline and supported Node/platform matrix. Octocode's current native binary does not require this branch.

## Verify the generated interface

| Boundary | Required evidence |
|---|---|
| Rust to JS types | Generate declarations with the project's pinned toolchain and inspect the resulting `.d.ts`; validate behavior from JS as well. |
| Integers and sizes | Check the locked version's number/BigInt conversion for each exact Rust type. Test safe-integer edges, sign, range and coercion; a TypeScript declaration is not runtime validation. |
| `Option`, maps, enums and typed arrays | Check undefined/null/omitted-field behavior, keys, discriminants, copying and ownership. Do not infer a mapping from a similar Rust type. |
| Error channel | Preserve useful core failure reasons through the established N-API error mapping; test throws and Promise rejection from JS. |
| DTO versus native object | Use small by-value data for transfer or an owned native object for retained state according to caller needs; measure repeated boundary conversion. |

Keep generated declarations/loaders derived from their owning declarations. Use supported custom type annotations only when runtime behavior matches them; do not maintain a separate handwritten mirror that can drift.

## Execution and lifetime

- Use the binding's supported async/task mechanisms to avoid blocking the JS event loop. An `async fn` body can still run blocking CPU work; select a finite blocking job or owned CPU worker when needed.
- Inspect which executor/pool each API uses. Do not assume every binding task shares Tokio or libuv, or that cancelling a Promise stops native work.
- Keep environment-bound JS values on their permitted thread/lifetime. Use the binding's supported owned/thread-safe transfer API for cross-thread callbacks; account for callbacks during teardown.
- Avoid flooding pools or retaining unbounded pending work. Follow the application's admission policy and test shutdown with native jobs still running.
- Follow [FFI panic and allocator rules](safety-and-ffi.md). An abort in a loaded addon terminates the host; panic containment must match the binding version and project policy.

## Packaging and verification

Use the existing binding template/CLI and package generation. Verify target, architecture, ABI, libc/Node-API compatibility, loader resolution, optional dependencies and the final archive's contents. A successful Rust compile is not proof that Node can load the addon.

Stage replacement artifacts according to the platform's loading/signing rules; do not overwrite an in-use mapped artifact in place. Load-test the final artifact in a child process on each supported environment required by the task. Preserve the owner's test configuration rather than copying another addon's disabled test/doctest settings.

Release metadata/provenance and publishing follow the project's release policy. Do not delete a user's lockfile or add a fallback loader merely to hide a missing platform artifact.

Sources: [napi-rs values](https://napi.rs/docs/concepts/values), [async functions](https://napi.rs/docs/concepts/async-fn), [thread-safe functions](https://napi.rs/docs/concepts/threadsafe-function), [napi-rs source](https://github.com/napi-rs/napi-rs). The pinned version and generated artifact decide applicability; website examples may describe a newer release.
