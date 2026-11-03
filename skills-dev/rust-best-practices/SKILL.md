---
name: rust-best-practices
description: "Write or review Rust when ownership, types, crates, workspace structure, builds, testing, performance, memory or safety need a decision; includes CLI/TUI, subprocess, napi-rs and parsing boundaries."
---

# Rust Best Practices

Read the owning crate’s `Cargo.toml`, `src/` and target code. Identify the requested behavior, constraints and check, including the edition and MSRV. In Octocode, load `octocode-dev` first. For a settled mechanical edit, run the repository’s checks. Use a Task contract if delegating.

## Resources

Open the page for the decision at hand.

- Choose [idioms](references/idioms.md) for errors, borrowing or async cleanup and [types and patterns](references/types-and-patterns.md) for structs, enums, traits or pointers.
- Use [crate structure](references/crate-structure.md) for dependency direction and extraction, or [workspace](references/workspace.md) for manifests, features, MSRV and lockfiles.
- Measure compile time and target size with [build profiles](references/build-profiles.md). For runtime cost, RSS or allocation, use [performance and memory](references/performance-and-memory.md). Support a performance claim with a `--release` benchmark or profile.
- Use [safety and FFI](references/safety-and-ffi.md) for untrusted input, panics, threads and boundary invariants. Require a demonstrated need for `unsafe` and a written safety invariant per block. [napi](references/napi.md) covers JS signatures and platform packages.
- Open [CLI/TUI/subprocess](references/cli-tui-subprocess.md) for streams, exits and child cleanup, or [parsing and codegen](references/parsing-and-codegen.md) for source coordinates and generated contracts.
- Use the [Octocode guide](references/octocode-usage.md) to inspect source, [sources and crates](references/sources-and-crates.md) to check the official canon or a dependency, and [testing and tooling](references/testing-and-tooling.md) for the crate’s verification gates.

## Rules

Compare a consequential open choice with a viable alternative. Test the smallest unknown that could change the decision. Prefer std or an existing dependency. For a new crate, verify maintenance, license and API at upstream sources. Cite the official Rust source by name for idiom and language claims. Check authorization before adding dependencies, changing MSRV or edition, or updating lockfiles. Request a decision for uncovered scope.

In Octocode the generator owns public tool types and limits, native owns execution and response policy, and hosts convert values. Deliver complete evidence pages with executable continuations. Disclose terminal limits.

Format edits, preserve observable behavior during cleanup, and run the applicable Clippy, build and test gates. Understand a lint before changing behavior. Rebuild affected hosts after native changes and exercise the CLI or fresh MCP. Follow [output.md](output.md) for the chat recommendation, ADR, evidence, unmeasured claims and requested notes.

## Related skills

Use `octocode-research` for upstream evidence, `octocode-brainstorming` for unresolved scope or feasibility, `octocode-architect` for boundaries, `octocode-clean-agentic-code` for cleanup, `octocode-roast` for a smell inventory, `octocode-eval-benchmark` for a tracked measurement, and `octocode-skills` to change this skill.
