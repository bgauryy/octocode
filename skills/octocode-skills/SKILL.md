---
name: octocode-skills
description: "Load this skill first when asked to find, create, audit, improve, install or sync Agent Skills (SKILL.md). Check triggers, instructions, reference routes and standalone bundles."
---

# Octocode Skills

Identify the requested skill, target host and result. For discovery, start with the [search order](references/find-skills.md). For an edit, locate the canonical entry and the description the host actually serves. A review returns findings. Installation or provider changes require the user's authorization.

## Work loop

1. Record real requests the skill should handle, a nearby request another skill owns, and a request needing no skill. For a collection, inventory each entry and its authored resources.
2. Walk a common request and a conditional one from description → first action → reference → result. Fix misleading triggers, ambiguous choices and paths that fail when installed alone. Keep safeguards and completion inline.
3. Run the [checker](scripts/skill-review.mjs) with Node: `node <this-skill>/scripts/skill-review.mjs <target> --json`, or `--collections <root> --json` for a collection; add `--commands` to check documented flags against each script's `--help`, and `--tells` to list MUST/NEVER per file and sentences repeated across files. Resolve findings (`link-escapes-skill` is a link a standalone install loses), exercise changed commands and recheck standalone routes. Use host trials for activation claims.

## Decide placement

| Path | Include when |
|---|---|
| `SKILL.md` | Required: trigger, normal workflow, safeguards and result. |
| `references/<task>.md` | A branch needs a recipe or protocol; link its load condition directly from the entry. |
| `scripts/` | Any step that is an exact operation: parse, count, compute, validate, transform, diff, hash, page through results or call an API. Code does it; the text says when to run it and how to read the result. [Prefer code over prose](references/write-skills.md#prefer-code-over-prose). |
| `assets/` | A step consumes a template or static input. |
| `output.md` | A complex reusable result contract warrants a separate read; otherwise keep it inline. |
| Host-specific files | The actual host consumes them; follow its schema and add only needed fields. |

Create only needed files and folders. Skill text never replaces code: an exact operation described in prose becomes a script, and the step names its command. Keep an instruction only when it helps choose, execute or verify an action. Preserve safeguards, recovery and evidence when shortening; add no arbitrary quotas, probes or metadata.

## Resources

| When | Read or run |
|---|---|
| Find an existing owner | [Find skills](references/find-skills.md) |
| Author or refine discovery and instructions | [Write skills](references/write-skills.md) |
| Start a new skill from the entry skeleton | [Template](references/template.md) |
| Audit a collection, validate or compare before/after | [Check skills](references/check-skills.md) |
| Install, adapt fetched code or sync providers | [Install and sync](references/install.md) |
| Settings or credentials are required | [Configuration](references/configuration.md) |
| Deliver a detailed or saved review | [Result formats](output.md) |

Return **Ready** or **Needs work**, changes/findings, checks and unchecked scope. Done when the checker reports no errors, changed commands ran, and each behavioral claim has before/after host runs or the missing run is named. Distinguish editorial, structural and observed behavioral evidence; rate only against stated criteria.
