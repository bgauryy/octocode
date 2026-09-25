---
name: rust-best-practices
description: "Use when writing, structuring, reviewing, or hardening Rust code and choices are open: crate selection, error-handling shape, module/workspace layout, design pattern, cargo build/profile tuning, dependency hygiene, performance, or unsafe/security. Triggers include idiomatic Rust, which crate, project structure, cargo workspace, compile time, reduce allocations, async pitfalls, borrow checker, unsafe review. Not for non-Rust code or a settled mechanical edit."
---

# Rust Best Practices

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Write, structure, and harden Rust that a seasoned maintainer would sign off on — idiomatic, fast, and safe by construction.

Flow: `FRAME → INSPECT → APPLY → VERIFY`. FRAME the axis in play (crates, idioms, structure, pattern, build, deps, performance, safety); INSPECT the real code and manifests before advising; APPLY the smallest change that fixes the axis; VERIFY with the repo's own checks.

Reports: `<output>/rust-best-practices/`; scratch: `<output>/tmp/rust-best-practices/`. Chat-only advice stays in chat; source edits keep their named paths.

## Lobby rules
- Inspect before advising. Read the real `Cargo.toml`, `src/` layout, and the target code; do not prescribe against an imagined project. Match the crate's existing edition, MSRV, and conventions.
- Evidence over reputation for crate picks. Verify a crate's maintenance, license, and API against upstream source (via `octocode-research`), not registry stars alone. Prefer std or an already-present dependency before adding a new one.
- Anchor idiom and API claims to the official canon, not memory. The Rust API Guidelines and the Reference settle disputes; name the source (`references/canonical-sources.md`). Language rules come from the canon; specific-crate behavior comes from upstream source.
- `cargo clippy` and `cargo fmt` are the baseline, not the ceiling. Never hand-argue a lint clippy already decides; run it. Treat clippy findings as the first review pass.
- Never change observable behavior while "cleaning up" — for behavior-preserving cleanup and dead-weight removal defer to `octocode-clean-agentic-code`.
- Measure before optimizing. No performance claim without a `--release` benchmark or profile; refuse to optimize on intuition. Reserve `unsafe` for a proven need and gate every block with a written safety invariant.
- Keep edits within the requested axis and authorization. Adding a dependency, bumping MSRV/edition, or touching a lockfile needs explicit consent.

## Axis map — pick the reference for the open question

| Axis | Question in play | Reference |
|------|------------------|-----------|
| Sources | "what official doc settles this?" / where a claim is anchored | `references/canonical-sources.md` |
| Crates | "which library for X?" / "is this dep the right one?" | `references/crates.md` |
| Idioms | error handling, ownership, `Result`/`Option`, iterators, API shape | `references/idioms.md` |
| Patterns | newtype, typestate, builder, RAII, sealed trait, enum-dispatch | `references/design-patterns.md` |
| Structure | modules, files, workspace, layering, where a file belongs | `references/project-structure.md` |
| Build & deps | cargo profiles, compile time, feature/version hygiene, supply chain | `references/build-and-deps.md` |
| Performance | allocations, `Cow`/`SmallVec`, `rayon`, zero-cost, profiling | `references/performance.md` |
| Gotchas | clone spam, `String`/`&str`, borrow checker, async pitfalls, `.unwrap()` | `references/gotchas.md` |
| Safety | `unsafe`, Miri, `cargo-audit`/`deny`, untrusted input, ReDoS/OOM | `references/safety-and-security.md` |
| FFI/interop | Node addon (napi-rs), `cdylib`, C bindings, panic-across-boundary | `references/ffi-and-interop.md` |
| Subprocess/IO | spawning children (LSP/compilers/CLIs), zombies, reaping, bounded pipes | `references/subprocess-and-io.md` |
| Parsing/codegen | tree-sitter/oxc parsing, structural queries, byte-range rewrites, offsets | `references/parsing-and-codegen.md` |

## Smart routes — load only what the current step needs
- When an idiom, API-shape, or language claim needs an authoritative anchor — or to pick which official doc settles a question — load `references/canonical-sources.md` — the official Rust doc canon (API Guidelines, Reference, Nomicon, Cargo/rustup/edition books, domain guides) with URLs, offline access via `rustup doc`, and the toolchain baseline flow.
- When choosing or vetting a dependency, load `references/crates.md` — the curated per-need stack, the lib-vs-app split, and the "before you add a crate" gate.
- When shaping errors, ownership, conversions, `Result`/`Option` flow, or public API surface, load `references/idioms.md` — idiomatic conventions and the `thiserror`-vs-`anyhow` decision.
- When a design question calls for a named pattern (state machine, invariant enforcement, resource lifetime, extensible dispatch), load `references/design-patterns.md` — Rust-native patterns with when-to-use and the anti-pattern each replaces.
- When deciding module boundaries, file placement, crate splits, or workspace layout, load `references/project-structure.md` — `lib.rs`/`main.rs`/`mod` rules, layered layout, workspace anatomy, and file/hierarchy limits.
- When tuning `cargo` profiles, cutting compile time, auditing feature flags/versions/supply chain, or organizing a workspace's shared metadata/deps/lints, load `references/build-and-deps.md` — release + profiling profile set, dev-speed levers, dependency-hygiene tooling, and the `[workspace.package]`/`[workspace.dependencies]`/`[workspace.lints]` centralization pattern.
- When a hot path is slow or allocation-heavy, load `references/performance.md` — measure-first protocol, allocation cuts, parallelism, and zero-cost idioms.
- When code fights the borrow checker, spams `.clone()`, or mishandles async, load `references/gotchas.md` — anti-pattern catalogue with the idiomatic fix for each.
- When reviewing `unsafe`, validating untrusted input, or gating a release, load `references/safety-and-security.md` — unsafe review checklist, Miri, and the audit/deny CI gate.
- When building a Node addon, `cdylib`, or any C-ABI/binding surface — including its TypeScript types, npm distribution, or release automation — load `references/ffi-and-interop.md` — napi-rs async/pool rules, panic-across-boundary, crate types, shipping the generated loader + typed `.d.ts`, and the `create-npm-dirs`/`artifacts`/`pre-publish` + provenance + cross-compile release pipeline.
- When spawning child processes (language servers, compilers, CLIs) or reading their output, load `references/subprocess-and-io.md` — kill+reap, group-kill, OS memory caps, bounded pipes, and async-fs hygiene.
- When parsing source, running structural queries, or rewriting code by byte range, load `references/parsing-and-codegen.md` — parser reuse, predicate/offset safety, and the non-overlapping + re-parse-to-validate rewrite protocol.
- For upstream crate source, real-world patterns, caller graphs, or version-at-ref proof, use `octocode-research` — never treat registry metadata as code evidence.

## Related routes
- Use `octocode-research` to verify crate source, trace APIs, and find real-world usage; `octocode-clean-agentic-code` for behavior-preserving cleanup; `octocode-roast` for a smell inventory; `octocode-eval-benchmark` to measure a performance change; `octocode-skills` for changes to this folder. No scripts.
