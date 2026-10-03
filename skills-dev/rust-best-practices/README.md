# Rust Best Practices

Write, structure, and harden Rust that a seasoned maintainer would approve. The skill inspects the real code and manifests before it advises, and it checks crate claims against upstream source, not registry popularity.

- **Use when:** a Rust choice is open (crates, idioms, modeling, patterns, layout, profiles, tooling, tests, performance, memory, `unsafe`, CLI/TUI, napi-rs, subprocesses, parsing). Also borrow-checker fights, `.clone()` spam, and async pitfalls.
- **Not for:** non-Rust code, a settled mechanical edit, or behavior-preserving cleanup (`octocode-clean-agentic-code`).
- **Structure:** `SKILL.md` holds the rules and one skill map (trigger → reference); `references/` holds 12 pages, one per decision area. No scripts.
- **Maintainer check:** run the `octocode-skills` review against this folder before shipping.
