# Modules, crates and boundaries

Load for file placement, visibility, dependency direction or extracting a crate. Read the existing architecture and actual callers before changing the layout.

## Keep boundaries useful

- Put reusable behavior behind a testable library API; keep binary/transport entry points focused on adaptation. A small binary need not gain a library solely to follow a template.
- Keep items private unless callers need broader visibility. Use `pub(crate)` for internal APIs and deliberate public re-exports to avoid accidental commitment to folder structure.
- Follow the crate's module-file convention (`foo.rs` with children or `foo/mod.rs`). Both are valid. Split by responsibility and navigation needs, not an arbitrary file count.
- Keep policy independent of transport/storage when those boundaries exist. Define the smallest needed interface at the policy boundary; do not impose traits or dependency injection on every helper.
- A module is the starting point. Extract a crate for a stable capability, reusable API, platform boundary or meaningful dependency/build isolation. Measure build-cost claims; extra crates also add metadata, API and compilation costs.
- A proc-macro library uses its own crate. Isolate raw C bindings behind a safe wrapper when that improves safety/ownership; wrapping one FFI call does not automatically require a new `-sys` package.
- Avoid a shared grab-bag crate that owns unrelated helpers. Name the capability and its consumers; break dependency cycles by correcting ownership or introducing the required lower-level interface.

## Public API consequences

| Choice | Evaluate |
|---|---|
| Expose a dependency type | Intentional integration value versus version/feature coupling; wrapping is not always necessary. |
| Generic API | Caller flexibility and optimization versus code size, compile time and bounds. |
| Dyn/concrete API | Runtime extension and smaller implementation surface versus dispatch/ownership needs. |
| `#[non_exhaustive]` or a sealed trait | Future evolution versus downstream construction, matching or implementation freedom. |
| Feature-gated capability | Additive API and supported combinations; one build's feature unification can hide missing edges. |

## Extract coherently

1. Map imports, callers, errors, feature flags, generated code and test support. Confirm actual symbol uses before claiming anything is unused.
2. Define the intended API and dependency direction; make the code self-contained at its current location first where practical.
3. Add the member and dependency through existing workspace conventions. Move code and all callers together; do not leave the shared tree temporarily uncompilable.
4. Add a re-export/compatibility layer only when the task's compatibility contract requires one.
5. Check the affected package independently and its consumers under supported features. Use `cargo metadata`/`cargo tree` to confirm the dependency graph, plus timings when performance motivated the split.

For manifest inheritance and feature behavior, read [workspace](workspace.md). For test layout, read [testing](testing-and-tooling.md). Octocode's ownership rules are in [local guidance](octocode.md).

Sources: [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html), [Cargo targets](https://doc.rust-lang.org/cargo/reference/cargo-targets.html), [API Guidelines](https://rust-lang.github.io/api-guidelines/).
