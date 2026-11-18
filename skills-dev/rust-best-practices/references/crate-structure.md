# Modules, crates and boundaries

Load for file placement, visibility, dependency direction, test placement, extracting a crate, or a crate that dominates build time. Read the existing architecture and actual callers before changing the layout.

## Clean modules

- Put reusable behavior behind a testable library API; keep binary/transport entry points focused on adaptation. A small binary need not gain a library solely to follow a template.
- Keep items private unless callers need broader visibility. Use `pub(crate)` for internal APIs and deliberate public re-exports to avoid accidental commitment to folder structure.
- Give each module one responsibility and a small surface. Split when a file mixes responsibilities or a reader must hold unrelated state to follow it, not at an arbitrary file count. Follow the crate's module-file convention (`foo.rs` with children or `foo/mod.rs`); both are valid.
- Keep policy independent of transport/storage when those boundaries exist. Define the smallest needed interface at the policy boundary; do not impose traits or dependency injection on every helper.
- Avoid a shared grab-bag crate or module that owns unrelated helpers. Name the capability and its consumers; break dependency cycles by correcting ownership or introducing the required lower-level interface.

## Clean crate architecture

A crate is Cargo's unit of compilation. On stable Rust one crate's frontend (parsing, macro expansion, type checking) runs on a single thread ([parallel rustc](https://blog.rust-lang.org/2023/11/09/parallel-rustc.html)), so one large crate on the critical path leaves cores idle however many the machine has. Large workspaces keep the graph wide ([Fast Rust Builds](https://matklad.github.io/2021/09/04/fast-rust-builds.html)): rust-analyzer splits `ide` across several crates ([architecture](https://github.com/rust-lang/rust-analyzer/blob/master/docs/book/src/contributing/architecture.md)), Servo split its script crate, and Zed keeps over 270 crates and still extracts more ([PR #52238](https://github.com/zed-industries/zed/pull/52238) split `language`).

Each rule states its reason. Numbers are CPU seconds (user + sys) from one 690k-line workspace; see [provenance](sources-and-crates.md#refinement-provenance).

| Rule | Because |
|---|---|
| Split a large crate along the layers it already enforces. Each crate owns one capability with its types and errors and depends only on crates below it. Before cutting, confirm the module graph is acyclic (a layer guard or a cycle query), then extract bottom-up. | Extracting the lower layers (93k of 458k lines) into their own crate cut a lower-layer unit-test edit from 218 to 34 CPU s, still 1 recompiled unit. The cold test build fell from 155 to 113 s wall, with the same 9,265 tests. |
| Prefer that split to moving unit tests into a test crate. Unit tests stay inside their crate; only tests that cross a new seam move up (at most 9.3k of 458k lines here). | Unit tests keep private access. Moving tests out instead leaves the library one serial unit and makes every item a test touches `pub`: about 495 items for one 70k-line module. A `pub` item no longer trips `dead_code`. |
| Give each family of related features (here, tool families) its own crate above one shared crate. A family never imports another family, and family crates never dev-depend on each other. | Families compile in parallel, and an edit recompiles its family and the crates above it. The largest lib-test unit drops from 458k lines to at most 70k. A sideways dev-dependency couples two families' test builds again. |
| Keep the top crate a facade: admission, dispatch and wiring only. | Every family edit recompiles the facade and everything above it, such as the CLI and its integration tests, so any logic kept there is rebuilt on every edit below it. While tools and dispatch shared one crate of 170–220k production lines, a one-line tool edit recompiled 8 units for 259–326 CPU s. |
| Put shared test fixtures in a dev-only support crate when they need no crate internals. Put them behind the owning crate's `test-support` feature when they do. Neither is ever enabled in a release graph. | Moving a mock HTTP server out of a library into a dev-only crate removed wiremock and six other crates from the dev build. The release graph stayed identical (447 units, 0 differences). A fixture inside a library feature compiles into every dev binary that unifies the feature. |
| Pass a build-wide value (build identity, clock, configuration) as a parameter from the crate that owns it, never as a global. | A lower crate that computes the value from the sources above it reruns its build script on every edit above it. A `OnceLock` installed at start-up adds an order dependence and an install step that every test repeats. A parameter has neither, and tests pass a fixed value. |
| A re-export or root `use` that keeps old paths compiling during a move is a bridge. Record it, rewrite its users to the real path, then delete it. | One extraction left 2,112 references going through bridges. Until those references are rewritten, each item has two names, and the next move inherits both. |
| Guard the crate graph with a test: a table of each crate's allowed workspace dependencies, checked against the manifests. Fail on any edge between families, from a family to the facade, and from the shared crate to a family. | A new edge becomes a reviewed decision rather than a stray `Cargo.toml` line. Once the crate guard enforced these edges, the module-level guards that checked the same edges were removed. |
| Keep each crate's public surface minimal. Widen an item only when a dependent names it, guided by the compiler (E0603, E0616, E0624). Log each widening so it can be narrowed later. | One extraction widened 872 items to `pub`. Under `private_interfaces = "deny"`, a widening cascades into every signature that names the item, and a reachable `pub` item is invisible to `dead_code`. |

- A core crate does not depend on its transports, CLI, async runtime, serialization format, test helpers (those are dev-dependencies) or any crate above it. If it needs one, the seam is in the wrong place.
- Keep serialization at the edge. In rust-analyzer only the `rust-analyzer` crate knows about LSP and JSON: to expose an `ide` type, it adds a serializable counterpart there and converts. Otherwise put serde derives behind a feature, as ruff, oxc and biome do. Heavy derives and generic-heavy code in a crate everything depends on put their cost on every build's critical path; see [compile cost](code-size.md).
- Generate code ahead of time and check it in, rather than expanding a proc macro on every build: oxc's `#[generate_derive]` generates impls ahead of time, and swc pre-generated its visitors to improve compile time ([PR #9262](https://github.com/swc-project/swc/pull/9262)). Deduplicate, derive by role and split generated types as [compile cost](code-size.md#generated-code) measures.
- A proc-macro library uses its own crate. Isolate raw C bindings behind a safe wrapper when that improves safety/ownership; wrapping one FFI call does not automatically require a new `-sys` package.
- New code goes in the crate that owns its seam, or in a new leaf crate, never into the crate on the critical path because it is convenient. An extra crate costs a manifest, metadata and an API; an oversized one costs a serial frontend on every edit.

### Size signals

Read `cargo build --timings` (and `cargo test --no-run --timings`) by CPU time when the machine is loaded:

| Signal | Action |
|---|---|
| A crate starts the critical path and other crates wait for it | Split it so dependents wait only for the part they use. A leaf crate holding the three generated types a lower crate named let that crate start at 0.4 s instead of 17.2 s. |
| A unit's frontend time far exceeds its codegen time | Cut its derives, generics and generated code ([compile cost](code-size.md)), or split it; more codegen units will not help. |
| One crate's unit-test build is the critical path of the test build | Split the crate along its layers (above). Move to a test crate only the tests that need just the public API or cross a seam. |
| A test-only or one-module edit recompiles a large crate and its dependents | Check build-script inputs ([needless rebuilds](build-profiles.md#stop-needless-rebuilds)) and that the test lives in its own file (below), then the crate boundary. |

## Place tests

| Test | Place | Why |
|---|---|---|
| Private logic of a crate | A `#[cfg(test)] mod tests;` declaration beside the code, with the body in its own file (`foo/tests.rs`). Never an inline `mod tests { … }` or a `#[test]` function in the implementation file. | Tests keep private access. File-granular tools can then tell test code from production: build scripts that hash non-test files, `rerun-if-changed`, codemods and review diffs. Here a test edit inside an implementation file changed the build identity and rebuilt the 455k-line library and the CLI. The same test in its own file recompiles 1 unit. |
| Tests that use only the public API or cross a crate seam | A test crate, or the facade above both crates | The library compiles once as an ordinary rlib, which a build with the same profile settings and features reuses, and the tests compile on top. rust-analyzer moved hir-ty's tests into the leaf crate `hir-ide` ([PR #23181](https://github.com/rust-lang/rust-analyzer/pull/23181)); Servo keeps [`tests/unit/script`](https://github.com/servo/servo/tree/main/tests/unit/script). If such tests need internals, expose them through one narrow `#[doc(hidden)]` module (Servo's `script::test`), not by making items `pub`. |
| Integration tests of the public API | One binary per crate: `tests/it/main.rs` ([uv](https://github.com/astral-sh/uv/tree/main/crates/uv/tests/it)) or `tests/testsuite/main.rs` ([cargo](https://github.com/rust-lang/cargo/tree/master/tests/testsuite)) declares the other files as modules | Each top-level `tests/*.rs` is a separate binary to compile and link; merging cargo's made its test suite compile 3× faster ([Delete Cargo Integration Tests](https://matklad.github.io/2021/02/27/delete-cargo-integration-tests.html)). Check no module disappeared after merging. |
| Fixtures and helpers shared by test crates | A dev-only support crate (uv's `uv-test`), or the owning crate's `test-support` feature for fixtures that need its internals (rule above) | One compiled copy for every test crate, and none in release. |
| Data-driven or spec suites | A `harness = false` binary that runs the cases ([Deno's `tests/` crates](https://github.com/denoland/deno/tree/main/tests)) | One runner over data files instead of compiled test functions; see [testing](testing-and-tooling.md#write-trustworthy-tests). |

## Public API consequences

| Choice | Evaluate |
|---|---|
| Expose a dependency type | Intentional integration value versus version/feature coupling; wrapping is not always necessary. |
| Generic API | Caller flexibility and optimization versus code size, compile time and bounds ([compile cost](code-size.md)). |
| Dyn/concrete API | Runtime extension and smaller implementation surface versus dispatch/ownership needs. |
| `#[non_exhaustive]` or a sealed trait | Future evolution versus downstream construction, matching or implementation freedom. |
| Feature-gated capability | Additive API and supported combinations; one build's feature unification can hide missing edges. |

## Extract coherently

1. Map imports, callers, errors, feature flags, generated code and test support. Confirm actual symbol uses before claiming anything is unused.
2. Define the intended API and dependency direction; make the code self-contained at its current location first where practical.
3. Add the member and dependency through existing workspace conventions. Move code and all callers together; do not leave the shared tree temporarily uncompilable.
4. For a move of more than a few files, write it as a replayable script, not hand edits. The script holds a path map; rewrites paths, including use trees, `super::` chains that cross the new crate root, and `#[path]` mounts; and generates manifests. It also runs a compiler-guided visibility loop that widens only the items E0603, E0616, E0624 and E0451 name, logging each one. Develop it on a copy while other work lands, then replay it on the fresh tree. A second run must change nothing, and the script must exit non-zero, naming the file, when an expected anchor is gone. Because: one crate split replays in 1 min 46 s over 1,073 files with 0 unresolved paths, so it lands on the current tree instead of merging hours of hand edits.
5. Add a re-export/compatibility layer only when the task's compatibility contract requires one, and treat it as a bridge to delete (rule above).
6. Check the affected package independently and its consumers under supported features. Use `cargo metadata`/`cargo tree` to confirm the dependency graph, and update the crate-edge guard and the shared test selection in the same change. When build time motivated the split, compare `--timings` by CPU time and confirm the critical path moved.

For manifest inheritance and feature behavior, read [workspace](workspace.md). For writing tests, read [testing](testing-and-tooling.md). Octocode's ownership rules are in [local guidance](octocode.md).

Sources: [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html), [Cargo targets](https://doc.rust-lang.org/cargo/reference/cargo-targets.html), [API Guidelines](https://rust-lang.github.io/api-guidelines/), [rustc `private_interfaces`](https://doc.rust-lang.org/rustc/lints/listing/warn-by-default.html#private-interfaces).
