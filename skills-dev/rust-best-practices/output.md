# Report the result

Lead with the implemented behavior, recommendation or reproduced finding. Include only detail that helps the recipient assess it.

| Task | Include |
|---|---|
| Implementation or refactor | What changed, why, checks and material remaining risks. |
| Review or diagnosis | Location, trigger, observed consequence, evidence and the smallest useful repair. Separate facts from hypotheses. |
| Consequential design choice | Context, decision, viable alternative and consequences; use an ADR only when the project or task needs one. |
| Performance work | Workload, toolchain/profile/features, baseline, changed result and measurement limits. Mark unmeasured benefits as unmeasured. |
| Blocked verification | Exact unrun check, cause and what remains unknown. Never equate compilation or a successful exit with complete behavioral coverage. |

Cite the source that decides a non-obvious language/API claim and use the dependency's applicable version. A routine patch does not need citations for every Rust idiom. Report command exit status and relevant test selection without dumping logs.

Save a separate note only when requested or required by the project. Follow the project's destination convention; do not assume a host-specific report renderer, another skill or an Octocode directory is installed. For optional presentation conventions, see [presentation](references/presentation.md); for protocol definitions, see the [catalog](references/protocol-catalog.md#catalog).

For requested Octocode notes, default to `<workspace>/.octocode/rust-best-practices/`; task scratch belongs in `<workspace>/.octocode/rust-best-practices/tmp/`. A user-named path takes precedence. `<workspace>` is the repository root.
