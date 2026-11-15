# artifactSearch questions

Use the round's frozen registry/version assignments. These are capability questions; source
oracles and expected answers belong in the campaign, separately from this file.

| ID | Question |
|---|---|
| ART-01 | For each assigned exact package version across npm, PyPI, crates, Maven, NuGet, Go, Packagist and RubyGems, report the registry version, source repository, source ref and whether that ref is verified or inferred. State every unknown rather than inventing a repository or commit. |
| ART-02 | For the assigned package, report all registry-stated dependencies, optional/peer dependencies, runtime requirements and license. Reconstruct the complete selected details through small response pages. |
| ART-03 | Search the assigned supported registry for the first three candidates matching the assigned terms, with name/version/description and enough identity for a resolve call. If fewer exist, report the exhausted scope; additional candidates beyond three are outside this question. |
| ART-04 | Resolve the assigned valid version range and tag. Compare the selected exact version with the registry's resolution at the recorded observation time. |
| ART-05 | Resolve a frozen nonexistent package and submit a missing-name request, wrong field type and unsupported ecosystem/operation combination. Distinguish no package from malformed input and provider refusal; report whether repair guidance is executable. |
| ART-06 | Run a mixed batch of valid and invalid rows, then repeat the same immutable resolution. Does each row retain its identity, and does an error contaminate a valid row or become a false cached success? |
| ART-07 | Perform the selected resolution and page walk on both local CLI and fresh MCP. Compare the complete selected facts, including empty/error results and cross-surface continuation behavior. |
| ART-08 | With an owned registry fixture, change metadata between calls and exercise no-store, Vary: *, repeated cache headers and a 304 that changes cache policy. Does the next lookup obey the origin and avoid stale fallback when reuse is forbidden? Repeat with an existing persistent home containing entries from the previous cache layout; prove that entries missing policy metadata are not reused. |

Compare with targeted registry JSON or the ecosystem's native package CLI. Count setup, request
and output text separately; registry metadata is not proof that a guessed Git tag exists.
