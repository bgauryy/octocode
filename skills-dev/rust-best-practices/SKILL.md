---
name: rust-best-practices
description: "Use when writing, structuring, reviewing, or hardening Rust and a choice is open: which crate, idiomatic errors/ownership, modeling structs/enums/traits, design patterns (good vs bad), splitting a workspace into npm-style crates, Cargo profiles and compile time, dev tools and testing, performance, memory and allocation (malloc/calloc equivalents), unsafe and supply-chain security, CLI or ratatui TUI apps, napi-rs Node addons with TypeScript types and per-platform npm packages, subprocesses, or parsing/codegen. Also for borrow-checker fights, clone spam, and async pitfalls. Not for non-Rust code or a settled mechanical edit."
---

# Rust Best Practices

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Write, structure, and harden Rust that a seasoned maintainer would sign off on — idiomatic, fast, and safe by construction.

Flow: `FRAME → INSPECT → APPLY → VERIFY`. FRAME the axis in play and load only its route below; INSPECT the real code and manifests before advising; APPLY the smallest change that fixes the axis; VERIFY with the repo's own checks.

Reports: `<output>/rust-best-practices/`; scratch: `<output>/tmp/rust-best-practices/`. Chat-only advice stays in chat; source edits keep their named paths.

## Lobby rules
- Inspect before advising. Read the real `Cargo.toml`, `src/` layout, and the target code; do not prescribe against an imagined project. Match the crate's existing edition, MSRV, and conventions.
- Evidence over reputation for crate picks. Verify a crate's maintenance, license, and API against upstream source (via `octocode-research`), not registry stars alone. Prefer std or an already-present dependency before adding a new one.
- When asserting an idiom or API rule, anchor it to the official canon, not memory. The Rust API Guidelines and the Reference settle disputes; name the source (`references/canonical-sources.md`). Language rules come from the canon; specific-crate behavior comes from upstream source.
- `cargo clippy` and `cargo fmt` are the baseline, not the ceiling. Never hand-argue a lint clippy already decides; run it. Treat clippy findings as the first review pass.
- Never change observable behavior while "cleaning up" — for behavior-preserving cleanup and dead-weight removal defer to `octocode-clean-agentic-code`.
- Measure before optimizing. No performance claim without a `--release` benchmark or profile; refuse to optimize on intuition. Reserve `unsafe` for a proven need and gate every block with a written safety invariant.
- Keep edits within the requested axis and authorization. Adding a dependency, bumping MSRV/edition, or touching a lockfile needs explicit consent.

## Smart routes — load only what the current step needs
Code design
- When a claim needs an authoritative anchor or you must pick which official doc settles it, load `references/canonical-sources.md`.
- When choosing or vetting a dependency, load `references/crates.md` — per-need canon and the add-a-crate gate.
- When shaping errors, ownership, conversions, iterators, or public API, load `references/idioms.md`.
- When modeling data (struct vs enum, constructors, primitives, generics vs `dyn`, smart pointers, porting OO objects), load `references/types-and-structs.md`.
- When a design question calls for a named pattern or a GoF/OO habit needs its Rust form, load `references/design-patterns.md` — good patterns and anti-patterns.
- When code fights the borrow checker, spams `.clone()`, or mishandles async, load `references/gotchas.md`.

Structure & build
- When placing modules/files or layering one crate, load `references/project-structure.md`.
- When deciding what goes in which crate, the API between crates, or extracting a module into a crate, load `references/crate-boundaries.md`.
- When creating/adding workspace crates or mapping an npm monorepo habit to Cargo, load `references/workspace-crates.md`.
- When writing the root `Cargo.toml` (shared package/deps/lints, edition, MSRV, lockfile, features), load `references/workspace-manifest.md`.
- When tuning `[profile.*]` or cutting compile time, load `references/build-profiles.md`.
- When setting up a dev loop or CI, or asking which tool does a job, load `references/dev-tooling.md`.
- When adding, organizing, or speeding up tests, load `references/testing.md`.

Speed, memory, safety
- When a hot path is slow or allocation-heavy, load `references/performance.md` — measure first.
- When RSS/peak memory, type size, or unbounded growth is the problem, load `references/memory.md`.
- When translating malloc/calloc/realloc/free, using zeroed/uninit buffers, or handing memory across FFI, load `references/allocation.md`.
- When reviewing `unsafe`, untrusted input, secrets, supply chain, or a release gate, load `references/safety-and-security.md`.

Apps & interop
- When building a command-line tool (args, stdout/stderr, exit codes, color, pipes, config, signals), load `references/cli.md`.
- When building a full-screen terminal UI, load `references/tui.md` — ratatui lifecycle, event loop, render tests.
- When crossing an FFI boundary (`cdylib`, C bindings, napi threads/panics), load `references/ffi-and-interop.md`.
- When choosing Rust types for `#[napi]` exports or owning the generated `.d.ts`, load `references/napi-types.md` — verified TS ↔ Rust mapping.
- When designing napi exports, laying out a binding crate, or shipping per-platform npm packages and releases, load `references/napi-packaging.md`.
- When spawning child processes or reading their output, load `references/subprocess-and-io.md`.
- When parsing source, running structural queries, or rewriting code by byte range, load `references/parsing-and-codegen.md`.
- For upstream crate source, real-world patterns, or version-at-ref proof, use `octocode-research` — never treat registry metadata as code evidence.

## Related routes
- Use `octocode-research` to verify crate source, trace APIs, and find real-world usage; `octocode-clean-agentic-code` for behavior-preserving cleanup; `octocode-roast` for a smell inventory; `octocode-eval-benchmark` to measure a performance change; `octocode-skills` for changes to this folder. No scripts.
