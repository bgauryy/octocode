# Research for an RFC

Use to identify evidence that can change the decision. `octocode-research` owns tool selection and verification.

| Question | Useful evidence |
|---|---|
| What happens today? | Current source, callers, contracts, tests, and affected users. |
| Which options are viable? | Local constraints, relevant prior art, primary documentation, and concrete alternatives. |
| Can a migration work? | Data and API compatibility, real consumers, failure modes, and reversible steps. |
| Does a dependency fit? | Its current release, maintained source, license, integration points, and applicable limitations. |
| Would the change help? | A reproducible baseline and relevant acceptance checks; use a benchmark when the claim needs measurement. |

Verify deciding claims in the original source. Search snippets are leads. Reconcile conflicting evidence or state the unresolved choice. Use `octocode-brainstorming` when the option space itself needs exploration.

Cite facts beside the design or rationale they support. A source list, if useful, is a section of the RFC. Keep meaningful evidence and material counterarguments; omit search queries, probe logs, agent receipts, and research chronology from the delivered document.

Next: close deciding gaps with [completeness](rfc-completeness.md), then write the [single-file RFC](../output.md).
