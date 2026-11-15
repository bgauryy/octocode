# securityCheck questions

Use the round's public package/ref assignments and record the observation time. A zero-result
lookup is a statement about the provider query, not proof that software is secure.

| ID | Question |
|---|---|
| SEC-01 | For each assigned package/version, report whether the provider lists it as affected, the complete advisory ID set, published fix ranges and any unassessed or withdrawn information. Compare the exact query with the provider's own record. |
| SEC-02 | Query the same assigned package through its package URL and compare its meaning with the package operation. Identify malformed URL, ecosystem or version inputs without silently substituting another package. |
| SEC-03 | Read the first advisory identified by SEC-01 in full. Report its identifiers, every affected package’s name and ecosystem, package URL (`purl`) when the provider supplies it, affected/fixed ranges, references and available severity provenance. Verify each provider's selected reference membership, order and available types independently, including records sharing an id. Preserve per-entry source and range metadata. Empty or missing fields in one record must never acquire another record's values; do not infer a missing severity or fix. Finish all response pages. |
| SEC-04 | Inspect the assigned public repository and public commit. Distinguish repository facts and published advisories from a commit's indexed vulnerability result. State what a zero or unknown commit lookup cannot establish. |
| SEC-05 | Submit a missing required field, a wrong type and a distribution-release option on a non-distribution ecosystem. Are refusals actionable, and does a mixed batch preserve valid row results? |
| SEC-06 | Repeat the same exact package-version or advisory-id query, and compare local CLI with fresh MCP. Advisory content can change despite a stable id: retain observation times and distinguish explicit cache state from unchanged output. Check whether changed views, errors and continuations retain the selected evidence and the correct provenance. |
| SEC-07 | With an owned OSV fixture, change a package query's advisory list and a hydrated record's body while retaining its listed identity. For no-store, no-cache and Vary: * responses, does every fresh query reach the provider instead of returning a cached assessment? Contrast a permitted fresh response, follow all selected pages, and check persistent-home migration. Record provider request counts separately from saved response-page replay. |

The expert baseline is the exact OSV/GitHub advisory query with a projection of the same facts.
Compare the documented selected representation: omitted empty collections are not proof of raw
upstream key presence or absence, and never authorize inheritance from another provider.
No secret-shaped data goes to a real provider. Provider availability, token state, request count
and origin-cache uncertainty are separate from the tool's correctness.
