---
name: rust-best-practices
description: "Use when writing, structuring, reviewing, or hardening Rust and a choice is open: which crate, idiomatic errors/ownership, modeling structs/enums/traits, design patterns (good vs bad), splitting a workspace into npm-style crates, Cargo profiles, compile time, target/ size and build/test rebuild churn, dev tools and testing, performance, memory and allocation (malloc/calloc equivalents), unsafe and supply-chain security, CLI or ratatui TUI apps, napi-rs Node addons with TypeScript types and per-platform npm packages, subprocesses, or parsing/codegen. Not for non-Rust code or a settled mechanical edit."
---

# Rust Best Practices

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies

```mermaid
flowchart TD
    F[FRAME: axis and constraint] --> L[LOAD its one page] --> I[INSPECT real code and manifests] --> D{Choice unresolved?}
    D -- yes --> C[COMPARE options and test the unknown] --> A[APPLY smallest change]
    D -- no --> A
    A --> V[VERIFY with repo checks]
    L -. "when a doc must settle a claim; choose or vet a crate" .-> R1["references/sources-and-crates.md"]
    L -. "for errors, ownership, async admission, cancellation and cleanup" .-> R2["references/idioms.md"]
    L -. "for struct vs enum, generics vs dyn, smart pointers, OO port, named pattern" .-> R3["references/types-and-patterns.md"]
    L -. "for modules, layers, what goes in which crate" .-> R4["references/crate-structure.md"]
    L -. "when adding a workspace crate; root Cargo.toml, MSRV, lockfile, features" .-> R5["references/workspace.md"]
    L -. "for [profile.*], compile time, target/ size, build/test rebuilds, cleanup" .-> R6["references/build-profiles.md"]
    L -. "for tests, dev loop, CI and merge gate, which tool" .-> R7["references/testing-and-tooling.md"]
    L -. "for slow hot path, RSS, type size, malloc/calloc" .-> R8["references/performance-and-memory.md"]
    L -. "for unsafe, untrusted input, supply chain, cdylib, napi threads/panics" .-> R9["references/safety-and-ffi.md"]
    L -. "for #[napi] types, .d.ts, per-platform npm packages" .-> R10["references/napi.md"]
    L -. "when building CLI args/streams/exit codes, ratatui TUI, child processes" .-> R11["references/cli-tui-subprocess.md"]
    L -. "for generated tool contracts, source parsing and byte-range rewrites" .-> R12["references/parsing-and-codegen.md"]
```

Skill map: solid edges are the flow; each dotted edge names the trigger that loads that `references/` page. Load only the page in play.

## Rules
- Read the real `Cargo.toml`, `src/` layout, and target code before advising; match the crate's edition, MSRV, and conventions.
- FRAME: name the requested behavior, constraint, owning crate, and success check. Inside Octocode, load `octocode-dev` first.
- COMPARE only an unresolved choice: compare the existing approach with one viable alternative. State evidence, cost, and a counterexample. Test the smallest unknown that can change the decision; mark unmeasured claims. Broader scope exploration routes to `octocode-brainstorming`.
- Prefer std or an already-present dependency. Verify a new crate's maintenance, license, and API against upstream source (`octocode-research`), not registry stars.
- Anchor idiom/API claims to the official canon (API Guidelines, Reference) and name it; crate behavior comes from upstream source.
- `cargo clippy` and `cargo fmt` are the first review pass: run them, never hand-argue a lint clippy decides.
- Never change observable behavior while cleaning up; behavior-preserving cleanup belongs to `octocode-clean-agentic-code`.
- No performance claim without a `--release` benchmark or profile. `unsafe` only for a proven need, each block with a written safety invariant.
- Stay within the requested axis. Adding a dependency, bumping MSRV/edition, or touching a lockfile needs explicit consent.
- Public tool types and limits follow the repository's contract generator. Native owns execution and response policy; host adapters convert values.
- Bound work without losing evidence: return a complete page and executable continuation. Disclose terminal limits; never silently clip results.
- VERIFY: use the repository's build and test commands. After native changes, rebuild hosts and exercise the actual CLI or fresh MCP.

## Related
`octocode-dev`: Octocode build and contract pipeline · `octocode-research`: source evidence · `octocode-brainstorming`: unresolved scope and feasibility · `octocode-architect`: boundary decisions · `octocode-clean-agentic-code`: cleanup · `octocode-roast`: smell inventory · `octocode-eval-benchmark`: measure a change · `octocode-skills`: change this folder.

## Output
One answer in chat. Save one note under `<output>/rust-best-practices/` only when the task asks to keep it. Scratch stays in `<output>/tmp/rust-best-practices/`.
