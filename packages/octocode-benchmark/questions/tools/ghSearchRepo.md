# ghSearchRepo: independently checkable questions

Use the current pinned CLI and a fresh MCP server. Load their schemas before choosing request fields. Questions contain no answer key; campaign oracles and outcomes are separate.

| ID | Case | Question |
|---|---|---|
| GHR-01 | Normal | Identify cli/cli and return its description, default branch and repository URL. Record the observation time for mutable metadata. |
| GHR-02 | Advanced | List the public vitejs repositories including archived ones, and independently search that owner for repositories with the vite topic sorted by stars. Walk every selected page with a two-row window; retain repository identities and the requested ranking key. |
| GHR-03 | Empty | Search public repositories owned by vitejs for the literal term octocode_benchmark_no_such_repo_20261114. Explain the actual searched scope and distinguish a successful empty response from provider failure. |
| GHR-04 | Invalid | Request operation get with owner cli but no repo, then send a misspelled field and an impossible operation/filter combination. Preserve each response and repair only as instructed by the schema. |
| GHR-05 | Sort/fields | Search vitejs repositories separately by forks, updatedAt and relevance, explicitly requesting the deciding field when available. Check that each ranking agrees with the equivalent provider selection and that old sort aliases are refused with the current name. |

For each case save the literal input, natural CLI stdout and stderr, MCP JSON-RPC request and response, exit status, binary identity and selected source identity. Finish emitted response continuations before tool/section continuations and copy their inputs unchanged. Attempt cursor-only resumption on the other surface and refusal of a changed request. A provider error is not an empty answer.

For paired efficiency measurements, use the same selected evidence with ordinary gh, lean projected gh and an explicitly TTL-cached gh api arm where immutable refs permit it. Keep logical request characters separate from environment/setup, preserve failed attempts, and obtain three paired passes in the exclusive timing window. Cache/provider coldness remains unknown without evidence.
