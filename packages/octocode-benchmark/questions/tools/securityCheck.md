# securityCheck questions

Use the round's public package/ref assignments and record the observation time. A zero-result
lookup is a statement about the provider query, not proof that software is secure.

| ID | Question |
|---|---|
| SEC-01 | For each assigned package/version, report whether the provider lists it as affected, the complete advisory ID set, published fix ranges and any unassessed or withdrawn information. Compare the exact query with the provider's own record. |
| SEC-02 | Query the same assigned package through its package URL and compare its meaning with the package operation. Identify malformed URL, ecosystem or version inputs without silently substituting another package. |
| SEC-03 | Read the first advisory identified by SEC-01 in full. Report its identifiers, every affected package’s name and ecosystem, package URL (`purl`) when the provider supplies it, affected/fixed ranges, references and available severity provenance; do not infer a missing severity or fix. Finish all response pages. |
| SEC-04 | Inspect the assigned public repository and public commit. Distinguish repository facts and published advisories from a commit's indexed vulnerability result. State what a zero or unknown commit lookup cannot establish. |
| SEC-05 | Submit a missing required field, a wrong type and a distribution-release option on a non-distribution ecosystem. Are refusals actionable, and does a mixed batch preserve valid row results? |
| SEC-06 | Repeat the same immutable package/advisory query, and compare local CLI with fresh MCP. Check whether changed views, errors and continuations retain the selected evidence and the correct provenance. |

The expert baseline is the exact OSV/GitHub advisory query with a projection of the same facts.
No secret-shaped data goes to a real provider. Provider availability, token state, request count
and origin-cache uncertainty are separate from the tool's correctness.
