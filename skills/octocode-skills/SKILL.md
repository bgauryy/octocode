---
name: octocode-skills
description: "Load this skill first when asked to find, create, audit, improve, install or sync Agent Skills (SKILL.md). Check triggers, instructions, reference routes and standalone bundles."
---

# Octocode Skills

Identify the requested skill, target host and result. First action: to find, follow the [search order](references/find-skills.md); to create, run that search for an owner, then copy the [template](references/template.md); to edit, locate the canonical entry and the description the host serves. Installation or provider changes require the user's authorization.

## Work loop

1. Record real requests the skill should handle, a nearby request another skill owns, and a request needing no skill. For a collection, inventory each entry and its authored resources.
2. Walk two recorded requests, one handled directly and one taking a reference branch or lacking an input: description → first action → routed resource → result. Mark each point where a reader would guess, stall or open an unrouted file, then fix misleading triggers, ambiguous choices and paths that fail when installed alone.
3. Run the [checker](scripts/skill-review.mjs): `node <this-skill>/scripts/skill-review.mjs <target> --json`, or `--collections <root> --json` for a collection; add `--commands`, `--tells` and `--sizes` as [run checks](references/run-checks.md) describes. Resolve findings (`link-escapes-skill` is a link a standalone install loses), exercise changed commands and rewalk both requests standalone. Use host trials for activation claims.

## Decide placement

| Path | Include when |
|---|---|
| `SKILL.md` | Required: trigger, normal workflow, safeguards and result. |
| `references/<task>.md` | A branch needs a recipe or protocol; link its load condition directly from the entry. |
| `scripts/` | A step is an exact operation (parse, count, validate, transform, page, call an API): code does it; the text says when to run it and how to read the result. [Prefer code over prose](references/code-over-prose.md). |
| `assets/` | A step consumes a template or static input. |
| `output.md` | A complex reusable result contract warrants a separate read; otherwise keep it inline. |
| Host-specific files | The actual host consumes them; follow its schema and add only needed fields. |

Create only the files and folders a row above calls for. When shortening, preserve safeguards, recovery and evidence; add no arbitrary quotas, probes or metadata. Editing rules: [lobby and files](references/write-skills.md#lobby-and-files).

## Resources

| When | Read or run |
|---|---|
| Author or refine instructions and layout; the name or description; a step that should be a script | [Write skills](references/write-skills.md); [skill name and description](references/skill-description.md); [code over prose](references/code-over-prose.md) |
| Audit a collection, validate or compare before/after; run the checker or exercise behavior | [Check skills](references/check-skills.md); [run checks](references/run-checks.md) |
| Install, adapt fetched code or sync providers | [Install and fetch](references/install.md); [sync a local skill](references/sync-skill.md); [skill-sync](scripts/skill-sync.mjs) resolves sources, lists host folders and syncs a local skill (a plan until `--approve`) |
| Settings or credentials are required | [Configuration](references/configuration.md) |
| Save or render a review | [Saved report](output.md#saved-report) |

Return **Ready** or **Needs work**, changes/findings, checks and unchecked scope, shaped per [response](output.md#response). Done when the checker reports no errors, changed commands ran, and each behavioral claim has before/after host runs; otherwise report the missing run as an unchecked gap.
