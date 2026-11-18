# Rust Best Practices

An agent-neutral workflow for implementing, debugging and reviewing Rust. It starts from the actual repository, compiler/edition/MSRV and enabled features, then routes to the relevant decision.

- `SKILL.md`: entry, work loop, core rules and conditional reference map.
- `basics.md`: concise ownership/correctness refresher and common misconceptions.
- `references/`: ownership, errors and clean code; types; concurrency/cancellation; crate architecture and test placement; Cargo workspaces, profiles, rebuild causes and settings measured and rejected; compile cost of generic and generated code; performance; tests, runners and the tools that enforce clean code and architecture; safety/FFI; and conditional CLI/parser/napi guidance. `octocode.md` isolates this repository's policies; `sources-and-crates.md` names primary authorities, dependency checks and where the measured numbers come from.
- `output.md`: outcome, evidence, verification scope and material limits without requiring a report for routine work.

Use any available agent tools; Octocode and neighboring skills are optional outside this repository. Shared research/config/presentation resources are generated from `skills/common` and must not be hand-edited. Their presence does not make a Rust change depend on Octocode credentials.

Maintainers: run the `octocode-skills` structural review and the repository's `skills:check` and applicable docs checks. Exercise representative tasks separately; passing structure alone does not establish agent activation or behavioral correctness.
