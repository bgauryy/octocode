# GitHub Research v2 — Task Acceptance

Admission was performed against the shared comparison validity gates and the v2
equivalence contract in [`README.md`](README.md). "Pass" means the task has one
primary remote-GitHub capability, a non-null independently verified oracle,
finite scope, deterministic budget, partial credit, and an explicit
contamination/re-verification policy.

## Per-task record

| Q | Stable taskId | Primary category | Difficulty | Provenance | Admission |
|---|---|---|---|---|---|
| Q1 | `ghrv2-route-regex-builder` | code search | easy | vs-gh-rtk Q2, WebFetch raw source | PASS |
| Q2 | `ghrv2-is-repo-absence` | repository discovery + negative evidence | medium | vs-gh Q6, raw export-surface check | PASS |
| Q3 | `ghrv2-flask-route-history` | commits/history | hard | vs-gh Q5, raw source/history check | PASS |
| Q4 | `ghrv2-zustand-fix-pr-state` | PR state metadata | easy | vs-gh-rtk Q4, independent PR/source check | PASS |
| Q5 | `ghrv2-vue-pr-diff-review` | PR code/diff review | hard | vs-gh-rtk Q3, independently reviewed fixed PR | PASS |
| Q6 | `ghrv2-express-router-trace` | cross-repository trace | hard | vs-gh Q4, raw package/source checks | PASS |
| Q7 | `ghrv2-zustand-next-contract` | cross-repository comparison | medium | vs-gh-rtk Q1, raw source/manifests | PASS |
| Q8 | `ghrv2-vscode-keybinding-dispatch` | large-repository search | medium | vs-gh-rtk Q5, raw current source | PASS |
| Q9 | `ghrv2-fastify-lifecycle` | docs/source bounded fetch | medium | vs-gh-rtk Q9, authoritative docs/source | PASS |
| Q10 | `ghrv2-axios-entry-chain` | repository discovery + entry trace | medium | vs-gh Q7, repository metadata/raw manifest (oracle corrected 2026-08-03) | PASS |
| Q11 | `ghrv2-esbuild-process-boundary` | repository discovery + runtime trace | medium | vs-gh-rtk Q8, API language/raw source | PASS |
| Q12 | `ghrv2-node-stream-event-wiring` | multi-file bounded fetch | hard | vs-gh-rtk Q7, raw source check | PASS |
| Q13 | `ghrv2-redis-bitfield-security` | issue→PR diff trace | hard | vs-gh-rtk Q10, API PR + raw source | PASS |
| Q14 | `ghrv2-deepagents-oolong-pr-review` | live PR review | hard | live PR #4338, api.github.com verification 2026-08-03 | PASS |

Difficulty balance: 2 easy, 6 medium, 6 hard.

## Provenance rules

- Reused facts come only from the independently verified
  `octocode-vs-gh/ground-truth.json` and
  `octocode-vs-gh-rtk/ground-truth.json` records.
- The cross-repository draft's prompts, solver answers, and results were
  candidate material only. Its null oracle was not admitted.
- Mutable facts retain `reverifyBeforeRun: true`; fixed PR/issue objects retain
  their numbers and immutable historical metadata.
- A run-time value such as language bytes, branch contents, file sizes, or PR
  state is accepted only when independently checked and frozen before the
  control arm starts.

## Removed from prior admission

| TaskId | Reason |
|---|---|
| `ghrv2-redis-unstable-guard` | Rigid file-lookup: task reduces to "fetch this file, quote this condition" — no cross-file research depth; same Redis BITFIELD topic already covered by Q13 |
| `ghrv2-lodash-baseget-trace` | Contaminated: control arm answers from training data (marked in oracle); poor discriminator for tool capability |

## Rejected or materially rewritten source questions

| Source | Disposition | Reason |
|---|---|---|
| vs-gh Q1 | Rejected original; replaced by v2 Q1 | AST call counting is not equivalent remote-GitHub capability. |
| vs-gh Q3 | Rewritten as v2 Q6/Q7 | "Most recently merged" is a live selector; fixed PR #15035 provides stable metadata and diff review. |
| vs-gh Q4 | Rewritten as v2 Q9 | Removed npm lookup; dependency and destination repository are discoverable through GitHub source/repository operations. |
| vs-gh Q7 | Rewritten as v2 Q10 | Removed npm-registry resolution; repository discovery is performed on GitHub. |
| vs-gh Q8 | Removed (contaminated) | Repository discovery on lodash; control can answer from memory. |
| vs-gh Q9 | Rewritten as v2 Q13 | Removed symbols/minification scoring; task now grades bounded anchored fetches on the security issue. |
| vs-gh Q10 | Rewritten as v2 Q7 | Removed Octocode batching/cache and rate-limit advantage. |
| vs-gh-rtk Q6 | Rejected original | Open-ended architecture essay and optional bonus lacked one atomic primary capability. |
| vs-gh-rtk Q8 | Rewritten as v2 Q11 | Removed npm lookup; retained verified GitHub language and process-boundary facts. |
| cross-repo Q1–Q10 | Rejected as oracle sources | `UNVERIFIED_DRAFT` with null answers; solver convergence and draft results are not independent truth. |

## Independent verification gaps

No task has a null oracle. Residual drift is handled by the pre-run verification
contract:

- Q3 file path/hierarchy (may drift across Flask versions);
- Q4 PR state;
- Q7 branch contents and default-branch head;
- Q10/Q11 language byte totals;
- line anchors and default-branch heads on all mutable-source tasks.

If any of these cannot be independently reverified before a run, that task is
marked unavailable for that run; its oracle is not guessed or silently carried
forward.

## Curation history

| Pass | Date | Action |
|---|---|---|
| pass1 | 2026-08-03 | Removed 5 pure-lookup/duplicate tasks; added deepagents PR review; oracle fixes for axios, flask |
| pass2 | 2026-08-03 | Removed redis-branch-guard (rigid) and lodash-baseget (contaminated); updated sindresorhus/is to remove star-count; bank renumbered Q1–Q14 (14 questions) |
