# Rust Best Practices

Write, structure, and harden Rust that a seasoned maintainer would approve — idiomatic, fast, and safe by construction. Evidence-driven: it inspects the real code and manifests before advising, and verifies crate claims against upstream source rather than registry popularity.

## Use when

- Choosing a crate for a need, or vetting a dependency already in `Cargo.toml`.
- Shaping error handling, ownership, conversions, or a public API surface.
- Picking a design pattern (newtype, typestate, builder, RAII, sealed trait, enum dispatch) or replacing an OO/GoF habit and anti-pattern.
- Modeling data: structs, enums, traits, primitives, smart pointers; C-style allocation (malloc/calloc) done safely.
- Mapping TypeScript ↔ Rust types for napi-rs exports.
- Deciding module boundaries, file placement, crate splits, or workspace layout.
- Splitting code into small workspace crates the way a Node monorepo splits packages.
- Picking dev tools for the edit loop, tests, coverage, CI, or release.
- Cutting memory footprint: type size, ownership shape, bounded caches, allocator.
- Tuning `cargo` profiles, cutting compile time, or auditing features/versions/supply chain.
- Cutting allocations or speeding up a hot path (measure-first).
- Untangling borrow-checker fights, `.clone()` spam, `String`/`&str` confusion, or async pitfalls.
- Reviewing `unsafe`, validating untrusted input, or defining a release gate.
- Building a Node addon (napi-rs), a `cdylib`, or any C-ABI/binding surface, and shipping it as per-platform npm packages.
- Laying out and writing tests: unit/integration/doc, snapshots, property, fuzz, CLI.
- Building a CLI (clap, exit codes, stdout/stderr, pipes) or a ratatui TUI.
- Spawning child processes (language servers, compilers, CLIs) and reading their output safely.
- Parsing source (tree-sitter/oxc), running structural queries, or rewriting code by byte range.

Not for: non-Rust code, a settled mechanical edit, or behavior-preserving dead-weight removal (→ `octocode-clean-agentic-code`).

## Rules

- Inspect the real `Cargo.toml`, `src/` layout, and target code before advising; match the crate's edition, MSRV, and conventions.
- Prefer std or an already-present dependency before adding a crate; verify crate claims against upstream source, not stars.
- `cargo clippy` + `cargo fmt` are the baseline review pass — run them, don't hand-argue lints.
- Measure in `--release` before any performance claim; reserve `unsafe` for a proven need with a written safety invariant.
- Keep edits within the requested axis; adding a dependency, bumping MSRV/edition, or touching a lockfile needs consent.

## Workflow

```text
FRAME → INSPECT → APPLY → VERIFY
```

## Structure

- `SKILL.md` — lobby: rules, the axis map, and routes into references.
- `references/canonical-sources.md` — the official Rust doc canon (API Guidelines, Reference, Nomicon, Cargo/rustup/edition books, domain guides) that anchors idiom/API claims, plus the rustup/cargo toolchain baseline.
- `references/crates.md` — the vetted per-need crate stack and the "before you add a crate" gate.
- `references/idioms.md` — error handling, ownership, API surface conventions.
- `references/design-patterns.md` — Rust-native patterns, GoF → Rust table, and anti-patterns.
- `references/types-and-structs.md` — objects as struct+impl+traits, shape table, primitives, generics vs `dyn`, smart pointers.
- `references/allocation.md` — malloc/calloc/realloc/free → Rust, uninit memory, FFI ownership, allocation failure.
- `references/napi-types.md` — TS ↔ Rust type mapping for napi-rs, structs/classes/enums/unions, `.d.ts` control.
- `references/project-structure.md` — modules, files, workspace anatomy, layering, hierarchy limits.
- `references/workspace-crates.md` — npm-packages-style crate splitting: npm→Cargo map, flat `crates/*` layout, split rules, feature unification.
- `references/crate-boundaries.md` — separating packages well: where to cut, boundary API design, cycle breaking, layering enforcement, module → crate extraction.
- `references/dev-tooling.md` — toolchain pin, the cargo tool belt, aliases, and CI order.
- `references/memory.md` — memory measurement, type shrinking, arenas/indices, bounded growth, allocator.
- `references/build-profiles.md` — release/profiling/dist profiles, per-crate codegen, dev-loop compile-time levers, `target/` reclaim.
- `references/workspace-manifest.md` — `[workspace.package]`/`dependencies`/`lints` centralization, edition/MSRV/version rules, feature and lockfile hygiene.
- `references/performance.md` — measure-first, allocation cuts, parallelism, zero-cost idioms.
- `references/gotchas.md` — borrow checker, clone spam, `String`/`&str`, async pitfalls.
- `references/safety-and-security.md` — unsafe review, Miri, untrusted input, secrets/crypto, hardening lints, supply chain, release gate.
- `references/ffi-and-interop.md` — napi-rs, `cdylib`, panic-across-boundary, TypeScript integration, and the per-platform npm distribution + release-automation pipeline.
- `references/napi-packaging.md` — `#[napi]` export shape, binding crate layout, per-platform `package.json` (`os`/`cpu`/`libc`) + generated loader.
- `references/testing.md` — test layout, test kinds, determinism, and the test loop.
- `references/cli.md` — CLI contract: clap, output streams, exit codes, color/TTY, broken pipe, config, signals, tests.
- `references/tui.md` — ratatui TUI: terminal restore, state/update/view, non-blocking loop, rendering, tests.
- `references/subprocess-and-io.md` — spawning children without zombies/OOM: kill+reap, group-kill, bounded pipes.
- `references/parsing-and-codegen.md` — tree-sitter/oxc parsing, offsets, and safe byte-range code rewrites.

## Related skills

`octocode-research` (crate source, caller graphs, real-world usage) · `octocode-clean-agentic-code` (behavior-preserving cleanup) · `octocode-roast` (smell inventory) · `octocode-eval-benchmark` (measure a change) · `octocode-skills` (change this folder).

## Maintainer verification

Run the `octocode-skills` review against this folder before shipping.
