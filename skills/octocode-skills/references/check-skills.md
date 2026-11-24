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
| Scripts/assets | Real commands, dependencies, inputs, effects, output and failure/recovery. Inspect imports needed by the shipped operation. Every script has a documented command and answers `--help`. |
| Prose doing code's job | An exact operation (parse, count, compute, validate, transform, page, call an API) described for the agent to do by hand: replace it with a script and its command ([prefer code](write-skills.md#prefer-code-over-prose)). |
| Result | Evidence and unchecked scope are clear; readiness or ratings have stated criteria. |
| Standalone bundle | Required resources and imports ship without sibling skills or repository-only dependencies. |

Classify each resource as required, conditional or unrouted. Read authored instructions and script contracts. Compare repeated decisions and examples by purpose. Walk a common task and a conditional branch, including what the reader must open before acting. Check the same route from an isolated skill folder. A link can pass in a monorepo while failing for someone who installed just that skill; the checker warns on it (`link-escapes-skill`).

Apply the [placement rules](../SKILL.md#decide-placement). Merge repeated decisions and pages always needed together; keep distinct recipes conditional. Preserve permissions, identifiers, exact commands, exceptions, evidence, recovery and completion. Do not replace clear prose with a larger table or decorative diagram. Rewalk both tasks after changes. Moving text does not establish a size or efficiency improvement.

Check the text for slop with `node <this-skill>/scripts/skill-review.mjs <target> --tells --json`. Each `tell-repeated-sentence` names a sentence found in two or more files, with every `file:line`: keep one owner and link to it; fix a repeat between generated shared copies at their owner. Each `tell-must-never` counts one file's uppercase `MUST`/`NEVER` with their lines: keep those guarding safety, authority or a hard contract and rewrite the rest as plain imperatives. Reworded repeats, stacked hedges, contrast framing and fixed-count lists still need reading ([writing tells](writing-tells.md)).

## 3. Run structural checks

`<this-skill>` is the installed skill's absolute path; targets resolve from the working directory. Node is required.

```bash
node <this-skill>/scripts/skill-review.mjs <target> --json
node <this-skill>/scripts/skill-review.mjs --collections <collection> --json
node <this-skill>/scripts/skill-review.mjs <target> --commands --json   # also run each script's --help and check documented flags
node <this-skill>/scripts/skill-review.mjs <target> --tells --json      # also MUST/NEVER per file and sentences repeated across files
node <this-skill>/scripts/skill-review.mjs --self-test
```

Inspect `errorCount`, `warnCount` and each skill's `findings` (`level`, `code`, `message`; `link-escapes-skill` and the tell findings add `file` and `line`): errors exit 1; warnings alone exit 0. Resolve warnings or explain them. Explicit collections may be empty; missing roots and incomplete child skills still fail. Run `--self-test` after checker changes.

The checker covers common frontmatter, names, description limits, local paths/anchors, resource reachability, scripts without a documented command (`script-usage-missing`) or `--help` (`script-help-missing`), Markdown links that resolve outside the skill folder (`link-escapes-skill`), with `--commands` documented flags a script's `--help` does not list (`command-flag-unknown`), with `--tells` the two tells above, and selected shared drift. It does not prove trigger quality, instruction correctness, script behavior, external URLs, or all YAML, imports and shared copies. Inline result contracts, conditional two-column tables and generic output destinations are valid; review their meaning against the actual task and consumer.

In the Octocode repo run `node skills/common/build.mjs --check`, `node skills-dev/octocode-dev/scripts/dev.mjs skills:check`, and `node skills-dev/octocode-dev/scripts/dev.mjs docs:verify`. Repair drift at its owner, regenerate with `node skills/common/build.mjs`, then inspect the generated diff. Preserve concurrent work. Use the specification's [skills-ref validator](https://github.com/agentskills/agentskills/tree/main/skills-ref) for publication/advanced YAML when available; report if not run.

## 4. Exercise behavior and portability

- Run the checker with `--commands` to compare documented flags with live `--help`; run normal and failure cases in a disposable workspace within existing authorization. Links alone do not validate commands.
- Resolve each `link-escapes-skill` warning: bundle the file, link a public URL, or name the other skill in plain text. Then copy the skill without siblings/repository dependencies; check required imports and relevant helper help/self-tests, then walk the changed task. Do not mutate real provider installations to test packaging.
- For a substantial template change, have a fresh agent use it on a different task without the author's explanation. Check its first action, missing-input and failure behavior, resource choices and completion. A procedure, reference guide and branching tool task may need different shapes; do not grade them by matching headings or file counts.
- For activation, use fresh target-host sessions on the same requests before/after, holding model, tool policy, permissions and project constant. Record correct routes, misses, wrong routes and false triggers; repeat before claiming small gains. A description-only probe is not host activation. Use the `octocode-agentic` activation procedure if needed; respect existing cost/side-effect authorization.
- Measure discovery text, activated entry and selected-branch reads separately with `node <this-skill>/scripts/skill-review.mjs <target> --sizes --json` (UTF-8 bytes per stage), or a named tokenizer. Preserve safety and completion checks. Smaller text is not measured token, latency, comprehension or cost savings.

## 5. Decide

Return **Ready** or **Needs work** for the checked scope. For a collection, give a row per skill. Material findings need location, trigger, impact, smallest repair and verification. Separate structural facts, editorial judgment and observed behavior; name missing checks. Stop on verified acceptance or a blocker requiring authority/input.
