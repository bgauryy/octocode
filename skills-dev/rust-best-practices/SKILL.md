---
name: rust-best-practices
description: "Write or review Rust when ownership, types, crates, workspace structure, builds, testing, performance, memory or safety need a decision; includes CLI/TUI, subprocess and parsing boundaries, and napi-rs (for napi-rs projects; not used in this repo)."
---

# Rust Best Practices

Read the owning crate’s `Cargo.toml`, `src/` and target code. Identify the requested behavior, constraints and check, including the edition and MSRV. In Octocode, load `octocode-dev` first. For a settled mechanical edit, run the repository’s checks. Use a Task contract if delegating.

## Resources

Read only the row for the current decision. Use [basics](basics.md) for an initial Rust overview.

| When | Read | Why |
|---|---|---|
| Errors, borrowing or async cleanup | [Idioms](references/idioms.md) | Ownership and cleanup rules |
| Structs, enums, traits or pointers | [Types and patterns](references/types-and-patterns.md) | Choose the representation |
| Dependency direction or extraction | [Crate structure](references/crate-structure.md) | Cohesive ownership |
| Manifests, features, MSRV or lockfiles | [Workspace](references/workspace.md) | Workspace contract |
| Compile time or target size | [Build profiles](references/build-profiles.md) | Measure build cost |
| Runtime, RSS or allocation | [Performance and memory](references/performance-and-memory.md) | Prove with release benchmark/profile |
| Untrusted input, panics, threads or FFI | [Safety and FFI](references/safety-and-ffi.md) | Require demonstrated need and written invariant per `unsafe` block |
| napi-rs project | [napi](references/napi.md) | JS signatures/platform packages; not used in this repo |
| Streams, exits or child processes | [CLI/TUI/subprocess](references/cli-tui-subprocess.md) | Resource and terminal cleanup |
| Source coordinates or generated contracts | [Parsing and codegen](references/parsing-and-codegen.md) | Preserve source/contract ownership |
| Source or dependency evidence | [Octocode guide](references/octocode-usage.md), [sources and crates](references/sources-and-crates.md) | Exact reads and official canon |
| Verification gates | [Testing and tooling](references/testing-and-tooling.md) | Crate checks |

## Rules

Compare a consequential open choice with a viable alternative. Test the smallest unknown that could change the decision. Prefer std or an existing dependency. For a new crate, verify maintenance, license and API at upstream sources. Cite the official Rust source by name for idiom and language claims. Check authorization before adding dependencies, changing MSRV or edition, or updating lockfiles. Request a decision for uncovered scope.

In Octocode the generator owns public tool types and limits, native owns execution and response policy, and hosts convert values. Deliver complete evidence pages with executable continuations. Disclose terminal limits.

Format edits, preserve observable behavior during cleanup, and run the applicable Clippy, build and test gates. Understand a lint before changing behavior. Rebuild affected hosts after native changes and exercise the CLI or fresh MCP. Follow [output.md](output.md) for the chat recommendation, ADR, evidence, unmeasured claims and requested notes.

## Related skills

Use `octocode-research` for upstream evidence, `octocode-brainstorming` for unresolved scope or feasibility, `octocode-engineer` for boundaries, `octocode-clean-agentic-code` for cleanup, `octocode-roast` for a smell inventory, `octocode-agentic` for a tracked measurement, and `octocode-skills` to change this skill.
