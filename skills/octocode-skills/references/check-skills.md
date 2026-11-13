# Check skills from discovery to files

Load for reviews, collection audits and validation. For “all,” inventory and assess every skill separately, including author-owned resources beyond the entry's links; check generated copies against their owner.

## 1. Define the test before editing

Record requests and expected skill/action/output/check: ordinary and implicit intent, missing input, neighbor, no-skill and failure/recovery. Preserve original text and baseline results for revisions. Keep held-out requests separate when making activation claims.

## 2. Walk the route

| Surface | Check |
|---|---|
| Served discovery listing | Recognizable intent, neighbor boundary, no stale overrides. |
| Entry (`SKILL.md`) | First action, missing-input handling, deciding conditions, safeguards, usable result and completion without obligatory directory exploration. |
| References | Named load condition, predictive filename, direct route to critical detail; no prerequisite detours or orphan cycles. |
| Scripts/assets | Real commands, dependencies, inputs, effects, output and failure/recovery. Inspect imports needed by the shipped operation. |
| Result | Evidence and unchecked scope are clear; readiness or ratings have stated criteria. |
| Standalone bundle | Required resources and imports ship without sibling skills or repository-only dependencies. |

Classify each resource as required, conditional or unrouted. Read authored instructions and script contracts. Compare repeated decisions, diagrams and examples by purpose; a text match is only a lead. Walk a common task and a conditional branch; count prerequisite reads before the first useful action.

Apply the [placement rules](../SKILL.md#decide-placement). Merge repeated decisions and pages always needed together; keep distinct recipes conditional. Preserve permissions, identifiers, exact commands, exceptions, evidence, recovery and completion. Do not replace clear prose with a larger table or decorative diagram. Rewalk both tasks after changes. Moving text does not establish a size or efficiency improvement.

## 3. Run structural checks

`<this-skill>` is the installed skill's absolute path; targets resolve from the working directory. Node is required.

```bash
node <this-skill>/scripts/skill-review.mjs <target> --json
node <this-skill>/scripts/skill-review.mjs --collections <collection> --json
node <this-skill>/scripts/skill-review.mjs --self-test
```

Inspect `errorCount`, `warnCount` and each skill's `findings`: errors exit 1; warnings alone exit 0. Resolve warnings or explain them. Explicit collections may be empty; missing roots and incomplete child skills still fail. Run `--self-test` after checker changes.

The checker covers common frontmatter, names, description limits, local paths/anchors, resource reachability and selected shared drift. It does not prove trigger quality, instruction correctness, script behavior, external URLs, all YAML/imports/shared copies, or portability of links escaping the skill. Inline result contracts, conditional two-column tables and generic output destinations are valid; review their meaning against the actual task and consumer.

In the Octocode repo run `node skills/common/build.mjs --check`, `node skills-dev/octocode-dev/scripts/dev.mjs skills:check`, and `node skills-dev/octocode-dev/scripts/dev.mjs docs:verify`. Repair drift at its owner, regenerate with `node skills/common/build.mjs`, then inspect the generated diff. Preserve concurrent work. Use the specification's [skills-ref validator](https://github.com/agentskills/agentskills/tree/main/skills-ref) for publication/advanced YAML when available; report if not run.

## 4. Exercise behavior and portability

- Compare changed commands with live `--help`; run normal and failure cases in a disposable workspace within existing authorization. Links alone do not validate commands.
- Copy the skill without siblings/repository dependencies; check required links/imports and relevant helper help/self-tests, then walk the changed task. Do not mutate real provider installations to test packaging.
- For a substantial template change, have a fresh agent use it on a different task without the author's explanation. Check its first action, missing-input and failure behavior, resource choices and completion. A procedure, reference guide and branching tool task may need different shapes; do not grade them by matching headings or file counts.
- For activation, use fresh target-host sessions on the same requests before/after, holding model, tool policy, permissions and project constant. Record correct routes, misses, wrong routes and false triggers; repeat before claiming small gains. A description-only probe is not host activation. Use the `octocode-agentic` activation procedure if needed; respect existing cost/side-effect authorization.
- Measure discovery text, activated entry and selected-branch reads separately in bytes or a named tokenizer. Preserve safety and completion checks. Smaller text is not measured token, latency, comprehension or cost savings.

## 5. Decide

Return **Ready** or **Needs work** for the checked scope. For a collection, give a row per skill. Material findings need location, trigger, impact, smallest repair and verification. Separate structural facts, editorial judgment and observed behavior; name missing checks. Stop on verified acceptance or a blocker requiring authority/input.
