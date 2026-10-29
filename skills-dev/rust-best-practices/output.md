# Output

Follow the [shared presentation and HTML delivery rules](references/presentation.md); retain this skill’s artifact format.

One answer in chat, BLUF: the choice first, then Claim–evidence: why, and the check that proves it, each beside its source or command result. Name the page that owns the rule. Mark an unmeasured performance claim as unmeasured (Facts, assumptions, unknowns).

| Situation | Shape |
|---|---|
| Crate or design choice settled | ADR slots: context, decision, consequences (cost, counterexample) |
| Dependency, MSRV/edition, or lockfile change needs consent | Decision request: options, tradeoffs, recommendation; no change until the user answers |
| A claim or check was not run | Facts, assumptions, unknowns: say which; never present a guess as a fact |

Save one note under `<workspace>/.octocode/rust-best-practices/` only when the task asks to keep it. Scratch stays in `<workspace>/.octocode/rust-best-practices/tmp/`. `<workspace>` is the project root. A path the user names takes precedence. Edits to this skill stay in `skills-dev/rust-best-practices/`.

Deliver the recommendation only. Leave out probe output and process metadata.

Protocol definitions: [shared catalog](references/protocol-catalog.md#catalog).
