---
name: rust-best-practices
description: "Use when implementing, debugging, refactoring or reviewing Rust code: ownership, traits, errors, async, concurrency, unsafe/FFI, crate layout and architecture, Cargo build and test speed, MSRV, tests or measured performance. Includes CLI/TUI, parsing and napi-rs."
---

# Rust Best Practices

Start with the repository instructions, owning `Cargo.toml`, toolchain file, CI commands and affected code. Resolve inherited workspace settings. Identify the requested behavior, public compatibility promise, edition, minimum supported Rust version (MSRV), target and enabled features. If a setting is absent, record it as unknown rather than inventing a version.

Use the host's available search, file, symbol and execution tools. Octocode and neighboring skills are optional. For Octocode development, load `octocode-dev` and [repository guidance](references/octocode.md); apply those rules only in that repository.

## Work loop

| Stage | Action | Evidence to keep |
|---|---|---|
| Inspect | Trace the behavior through callers, types and error/resource boundaries. Read only relevant references below. | Source locations, active features and an observable success condition. |
| Reproduce | For a defect, make the smallest failing regression or reproducer before fixing it. For a design choice, compare the current approach with one viable alternative. | Actual failure, or the unknown that decides the choice. |
| Implement | Change the owning layer and its callers together. Prefer safe, concrete code and existing dependencies. Preserve behavior outside the requested change. | Why ownership, invariants and cleanup remain valid. |
| Verify | Run the focused check, then the repository's applicable formatting, lint and test gates. Exercise the real boundary when it changed. | Commands, selected features/targets, results and unchecked scope. |

For a settled mechanical edit, use the existing checks without inventing an experiment or design document. If execution is unavailable, provide a reviewable change and state which checks remain unrun.

## Resources

Read only the row for the current decision. Use [basics](basics.md) for a short ownership and correctness refresher.

| When | Read | Why |
|---|---|---|
| Errors, borrowing, conversions, public APIs, cleanup or lint suppression | [Idioms](references/idioms.md) | Ownership, typed failures, clean-code rules and panic policy |
| Threads, locks, atomics, async or cancellation | [Concurrency and async](references/concurrency-and-async.md) | Task ownership, backpressure and shutdown |
| Structs, enums, traits or pointers | [Types and patterns](references/types-and-patterns.md) | Choose the representation |
| Module or crate layout, dependency direction, test placement or a crate that dominates build time | [Crate structure](references/crate-structure.md) | Clean architecture: enforced layers, guarded crate edges, minimal surfaces |
| Manifests, features, MSRV or lockfiles | [Workspace](references/workspace.md) | Workspace contract and feature forks |
| Compile time, repeated rebuilds, concurrent builds or target size | [Build profiles](references/build-profiles.md) | Measure by CPU and recompiled units; settings measured and rejected |
| Heavy generics, derives, generated code or large futures | [Compile cost](references/code-size.md) | Shrink monomorphized and generated code, measured |
| Runtime, RSS or allocation | [Performance and memory](references/performance-and-memory.md) | Prove with release benchmark/profile |
| Untrusted input, unsafe code or FFI | [Safety and FFI](references/safety-and-ffi.md) | Safety obligations, trust boundaries and verification limits |
| napi-rs project | [napi](references/napi.md) | Version-specific JS types, task lifetimes and platform artifacts |
| Streams, exits or child processes | [CLI/TUI/subprocess](references/cli-tui-subprocess.md) | Resource and terminal cleanup |
| Source coordinates or generated contracts | [Parsing and codegen](references/parsing-and-codegen.md) | Preserve source/contract ownership |
| Language/API claim or dependency selection | [Sources and crates](references/sources-and-crates.md) | Primary authority and version-aware evaluation |
| Research with available Octocode tools | [Octocode usage](references/octocode-usage.md) | Exact reads and continuations; other host tools also work |
| Working in the Octocode repository | [Octocode development](references/octocode.md) | Local ownership, commands and shared-build rules |
| Test design, test runners, verification gates or clean-code tools | [Testing and tooling](references/testing-and-tooling.md) | Process-safe, data-driven tests; tools that enforce clean code and architecture |

## Rules

- Distinguish language guarantees, project policy and context-dependent recommendations. Check semantics against primary documentation; check third-party APIs against the locked version. Newer stable syntax is not automatically compatible with the project's MSRV or edition.
- Fix ownership and type design before adding clones, shared state, boxing or `unsafe` to satisfy the compiler. A justified clone or concrete type is often simpler than a new abstraction.
- Keep code clean: small cohesive modules, typed errors at library boundaries, cleanup owned by `Drop` guards, no process-global mutable state, clocks and limits passed in, tests in their own files, and `#[expect(lint, reason)]` instead of `#[allow]`. Keep the architecture clean: crates split along enforced layers with a guarded crate graph, serialization and generated code at the edges, a dispatch-only top crate, a minimal public surface per crate, fixtures only in dev-only crates or `test-support` features, build-wide values passed as parameters, temporary bridges deleted, and no growth of the crate that holds the build's critical path. The [clean-code tools](references/testing-and-tooling.md#tools-that-keep-code-and-architecture-clean) enforce what they can.
- Change build settings, crate layout or test infrastructure only on measured evidence: CPU (user + sys) and recompiled units, A/B/A/B, never wall time under load. Do mechanical refactors with a replayable script. Do not reintroduce a setting [measured and rejected](references/build-profiles.md#measured-and-rejected-here) without new evidence.
- Reuse std and established dependencies. Evaluate a new dependency's API, features, MSRV, license, maintenance and build-time effects. Proceed within the user's authorized scope; ask only when an unresolved decision materially changes compatibility or scope. Do not require fresh permission for routine Cargo-managed lockfile updates already implied by the task.
- Preserve errors and requested data. Apply the owning contract's limits and backpressure; use continuation, streaming or explicit refusal instead of silently returning a prefix. Do not invent quotas or deadlines.
- Understand diagnostics before changing code. Do not silence a lint, delete a test, weaken an invariant or lower a coverage floor to obtain a green result.

## Done

The requested behavior is implemented or the review finding is reproduced, affected callers agree, and applicable checks pass or have an explicit blocker. For unsafe, concurrent or boundary code, include its failure and cleanup paths. Report the change or finding, evidence, checks and material limits using [output.md](output.md). Performance claims need comparable measurements.

## Related skills

If installed, use `octocode-research` for upstream code evidence, `octocode-brainstorming` for an unsettled option space, `octocode-engineer` for broader design tradeoffs, `octocode-debugging` for runtime diagnosis, and `octocode-skills` for skill maintenance. Their absence does not block this workflow.
