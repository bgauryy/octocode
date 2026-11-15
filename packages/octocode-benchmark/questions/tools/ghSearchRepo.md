# ghSearchRepo: independently checkable questions

Use the [shared protocol](README.md) and [rating rubric](../../RATINGS.md).

| ID | Case | Question |
|---|---|---|
| GHR-01 | Normal | Identify cli/cli and return its description, default branch and repository URL. Record the observation time for mutable metadata. |
| GHR-02 | Advanced | List the public vitejs repositories including archived ones, and independently search that owner for repositories with the vite topic sorted by stars. Walk every selected page with a two-row window; retain repository identities and the requested ranking key. |
| GHR-03 | Empty | Search public repositories owned by vitejs for the literal term octocode_benchmark_no_such_repo_20261114. Explain the actual searched scope and distinguish a successful empty response from provider failure. |
| GHR-04 | Invalid | Request operation get with owner cli but no repo, then send a misspelled field and an impossible operation/filter combination. Preserve each response and repair only as instructed by the schema. |
| GHR-05 | Sort/fields | Search vitejs repositories separately by forks, updatedAt and relevance, explicitly requesting the deciding field when available. Check that each ranking agrees with the equivalent provider selection and that old sort aliases are refused with the current name. |
| GHR-06 | Cache controls | Against an owned provider fixture, issue five identical cold repository lookups, repeat them, and restart the client with the same home. Count physical requests and verify all five complete answers. Contrast fresh responses with no-store, Vary: *, expired validators, 304 metadata changes and refusal. Does request coalescing preserve current evidence and independent cancellation? Measure any extra wait for an unusable leader response. |
| GHR-07 | Request identity | Exercise the transport boundary with different authorization, media and custom headers, including repeated values. Can any saved body or resumed journal cross request identities? Retain request counts and synthetic identity markers, never credential values. |
