---
name: rust-best-practices
description: "Use when writing, structuring, reviewing, or hardening Rust and a choice is open: which crate, idiomatic errors/ownership, modeling structs/enums/traits, design patterns (good vs bad), splitting a workspace into npm-style crates, Cargo profiles and compile time, dev tools and testing, performance, memory and allocation (malloc/calloc equivalents), unsafe and supply-chain security, CLI or ratatui TUI apps, napi-rs Node addons with TypeScript types and per-platform npm packages, subprocesses, or parsing/codegen. Also for borrow-checker fights, clone spam, and async pitfalls. Not for non-Rust code or a settled mechanical edit."
---

# Rust Best Practices

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load a reference only when it changes the next action.

```mermaid
flowchart TD
    F[FRAME: pick the axis] --> L[LOAD its one page] --> I[INSPECT real code and manifests] --> A[APPLY smallest change] --> V[VERIFY with repo checks]
    L -. "which doc settles a claim; choose or vet a crate" .-> R1["sources-and-crates.md"]
    L -. "errors, ownership, iterators, public API; borrow-checker, clone spam, async" .-> R2["idioms.md"]
    L -. "struct vs enum, generics vs dyn, smart pointers, OO port, named pattern" .-> R3["types-and-patterns.md"]
    L -. "modules, layers, what goes in which crate" .-> R4["crate-structure.md"]
    L -. "add a workspace crate; root Cargo.toml, MSRV, lockfile, features" .-> R5["workspace.md"]
    L -. "[profile.*], compile time" .-> R6["build-profiles.md"]
    L -. "tests, dev loop, CI, which tool" .-> R7["testing-and-tooling.md"]
    L -. "slow hot path, RSS, type size, malloc/calloc" .-> R8["performance-and-memory.md"]
    L -. "unsafe, untrusted input, supply chain, cdylib, napi threads/panics" .-> R9["safety-and-ffi.md"]
    L -. "#[napi] types, .d.ts, per-platform npm packages" .-> R10["napi.md"]
    L -. "CLI args/streams/exit codes, ratatui TUI, child processes" .-> R11["cli-tui-subprocess.md"]
    L -. "parse source, structural queries, byte-range rewrites" .-> R12["parsing-and-codegen.md"]
```

Skill map: solid edges are the flow; each dotted edge names the trigger that loads that `references/` page. Load only the page in play.

Reports: `<output>/rust-best-practices/`; scratch: `<output>/tmp/rust-best-practices/`. Chat-only advice stays in chat.

## Rules
- Read the real `Cargo.toml`, `src/` layout, and target code before advising; match the crate's edition, MSRV, and conventions.
- Prefer std or an already-present dependency. Verify a new crate's maintenance, license, and API against upstream source (`octocode-research`), not registry stars.
- Anchor idiom/API claims to the official canon (API Guidelines, Reference) and name it; crate behavior comes from upstream source.
- `cargo clippy` and `cargo fmt` are the first review pass: run them, never hand-argue a lint clippy decides.
- Never change observable behavior while cleaning up; behavior-preserving cleanup belongs to `octocode-clean-agentic-code`.
- No performance claim without a `--release` benchmark or profile. `unsafe` only for a proven need, each block with a written safety invariant.
- Stay within the requested axis. Adding a dependency, bumping MSRV/edition, or touching a lockfile needs explicit consent.

## Pages
Load on the map trigger: `references/sources-and-crates.md` · `references/idioms.md` · `references/types-and-patterns.md` · `references/crate-structure.md` · `references/workspace.md` · `references/build-profiles.md` · `references/testing-and-tooling.md` · `references/performance-and-memory.md` · `references/safety-and-ffi.md` · `references/napi.md` · `references/cli-tui-subprocess.md` · `references/parsing-and-codegen.md`

## Related
`octocode-research`: upstream crate source and version-at-ref proof (registry metadata is not code evidence) · `octocode-clean-agentic-code`: cleanup · `octocode-roast`: smell inventory · `octocode-eval-benchmark`: measure a performance change · `octocode-skills`: change this folder.
