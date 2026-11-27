Load for reviews, collection audits and validation. For “all,” inventory and assess every skill separately, including author-owned resources beyond the entry's links; check generated copies against their owner.

# Check skills from discovery to files

## 1. Define the test before editing

Record requests and expected skill/action/output/check: ordinary and implicit intent, missing input, neighbor, no-skill and failure/recovery. Preserve original text and baseline results for revisions. Keep held-out requests separate when making activation claims.

## 2. Walk the route

| Surface | Check |
|---|---|
| Served discovery listing | Recognizable intent whose wording alone separates it from neighbors (no other skill names, no “Not for …”), no stale overrides. Rules: [skill name and description](skill-description.md). |
| Entry (`SKILL.md`) | First action, missing-input handling, deciding conditions, safeguards, usable result and completion without obligatory directory exploration. |
| References | Named load condition, predictive filename, direct route to critical detail; no prerequisite detours or orphan cycles. |
| Scripts/assets | Real commands, dependencies, inputs, effects, output and failure/recovery. Inspect imports needed by the shipped operation. Every script has a documented command and answers `--help`. |
| Prose doing code's job | A step describes an exact operation for the agent to do by hand (the script column of [code over prose](code-over-prose.md)): replace it with a script and its command. |
| Result | Evidence and unchecked scope are clear; readiness or ratings have stated criteria. |
| Standalone bundle | Required resources and imports ship without sibling skills or repository-only dependencies. |

Classify each resource as required, conditional or unrouted. Read authored instructions and script contracts. Compare repeated decisions and examples by purpose. Walk the two requests from the [work loop](../SKILL.md#work-loop), including what the reader must open before acting, from an isolated skill folder. A link can pass in a monorepo while failing for someone who installed just that skill; the checker warns on it (`link-escapes-skill`).

Repair by the [placement rules](../SKILL.md#decide-placement) and the editing rules in [lobby and files](write-skills.md#lobby-and-files), then rewalk both requests. Moving text does not establish a size or efficiency improvement.

Check the text for slop with `node <this-skill>/scripts/skill-review.mjs <target> --tells --json`. Each `tell-repeated-sentence` names a sentence found in two or more files, with every `file:line`: keep one owner and link to it; fix a repeat between generated shared copies at their owner. Each `tell-must-never` counts one file's uppercase `MUST`/`NEVER` with their lines: keep those guarding safety, authority or a hard contract and rewrite the rest as plain imperatives. Reworded repeats, stacked hedges, contrast framing and fixed-count lists still need reading ([writing tells](writing-tells.md)).

Steps 3 and 4: [run structural checks](run-checks.md#3-run-structural-checks), then [exercise behavior and portability](run-checks.md#4-exercise-behavior-and-portability).

## 5. Decide

Apply the completion condition that closes [SKILL.md](../SKILL.md) to the checked scope: every part met is **Ready**; any unmet part is **Needs work**. Report the verdict, findings and missing checks per [response](../output.md#response). Stop on verified acceptance or a blocker requiring authority/input.
