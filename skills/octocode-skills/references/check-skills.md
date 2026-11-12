# Check skills from discovery to files

Load for a review, collection audit, context reduction or validation after authoring. For “all,” inventory every skill and shipped resource, but assess each skill separately; do not infer collection quality from one passing entry.

## 1. Define the test before editing

Record representative requests and the expected skill, action, output and check. Include explicit and implicit requests, missing input, a near-neighbor, no-skill intent and a failure/recovery case. For revisions, preserve original text and the baseline result. Keep a held-out request set separate when making activation claims.

## 2. Walk the route

| Surface | Check | Failure to look for |
|---|---|---|
| Discovery listing | Actual served name/description against intended requests and neighbors | Stale overrides, vague triggers, false activation or missing implicit intent |
| Lobby (`SKILL.md`) | Starting action, constraints, branches, result and completion signal | Mandatory directory exploration, duplicated overview, missing next step |
| References | Predictive filenames and labels, load conditions, direct task routes | Generic names, chained prerequisite reads, two orphan files linking each other |
| Scripts and assets | Real entry point, dependencies, inputs, output, errors and recovery | Invented flags, absent templates, unreviewed side effects or hidden network/cost |
| Output | Receiver can use it and verify success; evidence and unchecked scope are explicit | Format without a consumer, unsupported readiness or numeric quality score |
| Standalone bundle | All required files and imports ship; sibling skills are optional | A repository checkout hiding missing dependencies or escaped local links |

Inventory every reference, script, asset and generated dependency. Classify it as required, conditional or unrouted. Read owned instructions and script contracts; verify shared copies against their owner instead of rewriting them. Follow imports when needed to check dependencies. For a full audit, inspect all author-owned resources, not just those the current entry happens to link.

Count prerequisite reads from the lobby to the deciding instruction, not directory nesting. A direct reference is one hop; a reference that only sends the agent elsewhere is a detour. Keep one decision per owner. Fix proven detours without flattening unrelated optional detail into the lobby.

### Check repetition and lobby placement

1. Inventory authored entry, reference and output text separately from generated copies. Compare repeated paragraphs, table rules, diagrams and examples by the decision they govern; a text match is only a lead.
2. Walk a common request and a conditional branch. Record what the agent must read before its first useful action, including constraints hidden behind references. Apply the [placement rules](../SKILL.md#decide-placement).
3. Merge repeated decisions at their first needed owner. Keep useful short links and examples with distinct purposes. Move only branch-specific detail out; promote only deciding rules in. Do not replace concise prose with a larger table or decorative diagram.
4. Rewalk both tasks after editing. Check that conditions, permissions, exact commands, evidence distinctions, recovery and completion still reach the same decisions. Report entry size and required branch reads separately; moving text is not deleting it or proving an efficiency gain.

| Content signal | Repair | Preserve |
|---|---|---|
| Repeated rule or explanation | Keep one reachable owner; link from other readers. | Scope, permissions, identifiers and exceptions. |
| Dense conditions mixed with actions | Use trigger → action → reason/check rows. | The condition that changes the action. |
| Branch, loop, handoff or state transition | Use a compact Mermaid diagram when clearer. | Edge conditions, failure paths and completion. |
| Exact values, choices or commands | Use a table or runnable example. | Units, syntax, prerequisites and expected results. |
| Already clear short prose, table or code example | Retain it. | Useful detail; do not add decorative visuals. |

## 3. Run structural checks

Use Node and the installed `octocode-skills` folder. In these commands, `<this-skill>` is its absolute path, `<target>` a skill folder and `<collection>` a collection root; targets resolve from the command's working directory.

```bash
node <this-skill>/scripts/skill-review.mjs <target> --json
node <this-skill>/scripts/skill-review.mjs --collections <collection> --json
node <this-skill>/scripts/skill-review.mjs --self-test
```

The checker reports `errorCount`, `warnCount` and per-skill `results`; errors exit 1, warnings alone exit 0. Inspect the payload, not only the exit code. Resolve warnings or report why they remain. `--collections` permits an existing empty collection; a missing root or incomplete child skill still fails. Run `--self-test` when changing the checker.

It checks common frontmatter forms, names, description limits, Octocode layout conventions, local Markdown paths/anchors, resource reachability and selected shared-file drift. It does **not** prove trigger quality, instruction correctness, script behavior, external URLs, all advanced YAML, all shared copies or all script imports. Cross-skill links escaping the folder are skipped; inspect them and test portability separately.

In the Octocode monorepo, from its root:

```bash
node skills/common/build.mjs --check
node skills-dev/octocode-dev/scripts/dev.mjs skills:check
node skills-dev/octocode-dev/scripts/dev.mjs docs:verify
```

Shared-file drift: edit the owner, regenerate with `node skills/common/build.mjs`, review the generated diff, then rerun. Do not overwrite another session's changes. For standards validation, use the specification's [skills-ref validator](https://github.com/agentskills/agentskills/tree/main/skills-ref) if available; report when it was not run rather than treating the local checker as a full YAML validator.

## 4. Exercise behavior and portability

- Run changed commands against live `--help`, then test normal and failure cases in a disposable workspace with only authorized effects. Static links are not command validation.
- Copy the skill to an isolated location without sibling skills or repository dependencies. Run its checker and relevant script help/self-tests there; exercise the changed task path. Inspect required links and imports, including paths the checker skips. Do not mutate real provider installations merely to test packaging.
- For activation, use fresh target-host sessions on the same prewritten requests before and after, with the same model, tool/permission policy and representative project. Record first skill loaded or none, action, result and failures. Count correct routes, misses, wrong routes and false triggers. Test intended models; repeat before claiming a small difference.
- A description-only selection probe is not host activation. Use the `octocode-agentic` skill-activation procedure when available for controlled runs. Paid or external runs require the applicable authorization. Without them, report activation as unverified.
- Measure discovery text, activated entry and optional branch reads separately in bytes or with a named tokenizer. Include the unchanged safety and completion checks. Smaller bytes are not measured token, latency or cost savings; more optional files can coexist with a cheaper common path.

## 5. Decide

Return **Ready** or **Needs work** for the checked scope. Give each material finding its location, trigger, impact, smallest repair and verification. Separate structural findings, editorial judgments and observed behavior; report missing checks explicitly. For collections, return a row per skill. Stop when checks establish the requested result or a named blocker requires authority or input; do not claim measured improvement from an editorial rewrite.
