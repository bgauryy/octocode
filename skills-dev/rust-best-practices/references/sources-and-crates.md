# Primary sources and dependency decisions

Load for a disputed language/API claim or dependency selection. Distinguish language guarantees, compiler behavior, crate contracts and recommended design. Project policy controls the local workflow; it cannot change language semantics.

## Choose the authority

| Question | Primary source | Applicability |
|---|---|---|
| Ownership and ordinary error handling | [The Book](https://doc.rust-lang.org/book/) | Explanatory introduction; check the project's compiler/edition. |
| Language rules and undefined behavior | [The Reference](https://doc.rust-lang.org/reference/) | Language authority; its UB/aliasing description is not a complete formal specification. |
| Unsafe design and FFI | [Rustonomicon](https://doc.rust-lang.org/nomicon/) | Explanation alongside the Reference and exact API safety contract. |
| Std guarantees and stabilization | [std](https://doc.rust-lang.org/std/) | Read the exact API, guarantees and “since” version. |
| Public API recommendations | [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) | Design guidance; do not treat every recommendation as a language requirement. |
| Features, manifests, profiles, MSRV and lockfiles | [Cargo Book](https://doc.rust-lang.org/cargo/) | Match the Cargo version and effective config. |
| Edition-sensitive behavior | [Edition Guide](https://doc.rust-lang.org/edition-guide/) | Check edition and compiler stabilization separately. |
| Lints | [Clippy](https://doc.rust-lang.org/clippy/) and [rustc lint listing](https://doc.rust-lang.org/rustc/lints/listing/index.html) | Inspect the lint's meaning, applicability and group before suppression. |
| Toolchain/components | [rustup Book](https://rust-lang.github.io/rustup/) | Toolchain selection is distinct from the package's MSRV promise. |
| Documentation tests | [rustdoc Book](https://doc.rust-lang.org/rustdoc/) | Enabled doctests need an executing runner. |
| Async/task lifecycle | [Tokio docs](https://docs.rs/tokio/latest/tokio/) or the actual runtime's upstream docs | Resolve to the version in `Cargo.lock`; no runtime is assumed. |
| UB checking and its limits | [Miri](https://github.com/rust-lang/miri) | Detector with execution/platform limitations, not proof of soundness. |
| Performance methods | [Rust Performance Book](https://nnethercote.github.io/perf-book/) | Author's methodology; benchmark results are workload-dependent. |
| Organizational practice | [Microsoft Rust Guidelines](https://microsoft.github.io/rust-guidelines/), [Canonical Rust practices](https://canonical.github.io/rust-best-practices/) | First-party practice, not universal Rust requirements. |
| JS/native bindings | [napi-rs docs](https://napi.rs/) and [source](https://github.com/napi-rs/napi-rs) | Locked binding/CLI versions and generated/runtime interface decide. |

Use `rustup doc` for installed offline documentation. If browsing is unavailable, identify the version/evidence gap; do not invent API availability or report a search as verification. For third-party APIs, `/latest/` is a discovery link: use the resolved version's docs/source for an actual change.

## Evaluate a dependency

1. Identify the required capability and why std or an already-used dependency does not meet it. Do not select a crate from a generic “best stack” table.
2. Read the resolved/candidate version's upstream API, changelog and support policy. Check MSRV, target support, enabled features, license and maintenance; a download count is not a quality guarantee.
3. Inspect the added graph and build-time execution: proc macros, build scripts, native code, TLS/backend choices and optional features. Existing advisory/license tools answer narrower questions than trustworthiness.
4. Compare the existing solution with the candidate on the deciding constraint. Benchmark performance claims; test public error/type interoperability and JS/FFI boundaries when applicable.
5. Make authorized manifest/lockfile changes through Cargo and review the diff. Do not upgrade unrelated dependencies or change compatibility promises as a side effect.

## Refinement provenance

Reviewed against primary documentation on **2026-11-12**: Vec growth and Arc auto-trait guarantees; additive Cargo features and MSRV; Tokio task/process cancellation; Miri's limitations; unsafe-block obligations; API evolution and profiling methodology. This review establishes source support, not measured runtime improvement or activation accuracy across every agent host. Recheck version-sensitive advice when applying it.
