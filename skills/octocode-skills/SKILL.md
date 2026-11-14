---
name: octocode-skills
description: "Use when finding, creating, improving, validating, installing or syncing Agent Skills (SKILL.md files): trigger descriptions, instructions, references and packaging. Use octocode-agentic for measured activation tests."
---

# Octocode Skills

Use this as the canonical skill-authoring structure; adapt it to the task and host. Identify inputs, authority and observable acceptance. Review without writes unless edits are requested; improve canonical sources. Discovery alone does not authorize installation.

## Work loop

1. Reuse the existing owner. Check intended, implicit, neighboring, no-skill and failure requests; preserve baseline results for comparisons.
2. Keep the first action, deciding conditions, safeguards and completion inline. Defer independent branch detail; merge references always read together. Regenerate shared copies at their owner.
3. Run the [checker](scripts/skill-review.mjs): `node <this-skill>/scripts/skill-review.mjs <target> --json` (`--collections <root>` for a collection). Inspect errors/warnings, exercise changed commands and verify standalone packaging. Structure alone proves no behavioral gain.

## Template

The smallest bundle is `<skill-name>/SKILL.md`. Replace placeholders and delete unused slots. Use steps for procedures, condition → action/check rows for reference or decision skills, and conditional resource links for independent branches. The skeleton is adaptable, not a required section order.

```markdown
---
name: <skill-name>
description: "Use when <recognizable task or situation>. <Distinctive result and essential boundary>."
---

# <Title>

<First action and required inputs. If a critical input is missing, retrieve it
or ask only for the blocking information. Relevant scope or permission boundary.>

1. <Task-specific action and its deciding condition.>
2. <Next action or branch; read its named reference only when needed.>
3. <Verify the result; on failure, recover within scope or report the blocker.>

Return <usable result or artifact, deciding evidence and unchecked scope>.
Done when <observable acceptance condition>.
```

## Decide placement

| Path | Include when |
|---|---|
| `SKILL.md` | Required: trigger, normal workflow, safeguards and result. |
| `references/<task>.md` | A branch needs a recipe or protocol; link its load condition directly from the entry. |
| `scripts/` | A repeated exact operation benefits from executable code. Document inputs, dependencies, effects and failure checks. |
| `assets/` | A step consumes a template or static input. |
| `output.md` | A complex reusable result contract warrants a separate read; otherwise keep it inline. |
| Host-specific files | The actual host consumes them; follow its schema and add only needed fields. |

Create only needed files and folders. Keep an instruction only when it helps choose, execute or verify an action. Preserve safeguards, recovery and evidence when shortening; add no arbitrary quotas, probes or metadata.

## Resources

| When | Read or run |
|---|---|
| Find an existing owner | [Find skills](references/find-skills.md) |
| Author or refine discovery and instructions | [Write skills](references/write-skills.md) |
| Audit a collection, validate or compare before/after | [Check skills](references/check-skills.md) |
| Install, adapt fetched code or sync providers | [Install and sync](references/install.md) |
| Settings or credentials are required | [Configuration](references/configuration.md) |
| Deliver a detailed or saved review | [Result formats](output.md) |

Return **Ready** or **Needs work**, changes/findings, checks and unchecked scope. Done when the checker reports no errors, changed commands ran, and each behavioral claim has before/after host runs or the missing run is named. Distinguish editorial, structural and observed behavioral evidence; rate only against stated criteria.
