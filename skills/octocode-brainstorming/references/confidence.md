# Evidence Confidence Markers

Load when scoring prior-art evidence strength for the `Evidence by Surface` section or a saved brief. Output template: `references/output.md`.

| Marker | Minimum evidence |
|---|---|
| strong | independent validated sources, or direct code/data plus strong activity/usage |
| moderate | one validated source plus corroborating evidence |
| weak | popularity/marketing/forum only, stale source, or no independent validation |

Every prior-art entry carries a marker. Search snippets are leads; cite fetched pages, exact code, package metadata, PRs, commits, or tests.
Marketing remains weak. Require an independent source or direct code/data before calling a claim proven. Present material contradictions; treat zero prior art as a risk, not a moat.

Decision labels route as follows: Build RFC → `octocode-rfc-generator`; Prototype First → test one unknown; Narrow → tighter user/problem; Park → weak evidence/timing; Do Not Build → prior art or risks dominate.
