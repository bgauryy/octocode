# Skill review

Load when you review, create, or update a skill, or fix findings. Why: map each code to the exact gap before you claim done.

```bash
node scripts/skill-review.mjs [<skill-dir | collection-dir>...] [--json]  # no args: nearest skills/ root
node scripts/skill-review.mjs --self-test                                # regression
```

- Exit `1` on any ERROR; WARN is advisory. A no-arg scan is relative to this copy (`.agents/skills/octocode-skills` scans `.agents/skills`).
- The script checks frontmatter, routes, schemes, length, hooks, prose, paths, and reachability; you make the judgment checks.
- `hooks-*` codes cover Claude-style `hooks:` frontmatter only; review Cursor and Codex native configs directly.

## ERROR (exit 1)

| Code | Requirement |
|---|---|
| `frontmatter-missing` | `SKILL.md` starts with YAML frontmatter |
| `name-mismatch` | `name` equals the folder name |
| `name-format` | `name` is 1–64 chars of `a-z0-9` with single inner hyphens |
| `compatibility-length` | `compatibility`, when present, is 1–500 chars |
| `description-missing` / `description-too-long` | non-empty `description`, ≤1024 chars |
| `missing-route` | every routed file or directory path exists |
| `link-outside-skill` | no `../dir/file`, `~/`, `file://`, or absolute path; a bare `../dir` argument or `<placeholder>` path is fine |
| `octocode-contract-stale` | current Octocode tool names and `octocode skill install/list/info` forms |
| `lobby-*-convention` | the lobby header convention holds |
| `scheme-contract` | each scheme entry is a flat `.json` file, one top-level object |
| `unused-file` | every shipped file is reachable from `SKILL.md`, `README.md`, or a used file; drop dev metadata and probes |

## WARN (assess)

| Code | Fix |
|---|---|
| `description-trigger` | lead with `Use when <trigger>` |
| `name-reserved` / `frontmatter-xml` | drop `anthropic` or `claude` from `name` and XML tags from `name` or `description`; Anthropic uploads reject them |
| `lobby-long` | over 150 lines: move detail, not core logic, into references |
| `lobby-map-large` | over 12 flow nodes (page leaves excluded): merge phases or move a loop into its page |
| `description-shape` | one `Use when …` sentence plus an optional `Not for …` boundary; no trigger list, no instructions |
| `description-voice` | no "I"/"you" and no mandate words (MUST, ALWAYS, NEVER, IMPORTANT, CRITICAL) |
| `readme-missing` | add `README.md`: overview, capabilities, how it works, install |
| `reference-h1` | one short H1 per reference |
| `reference-long` | over 100 lines: cut duplication; split only a page with two decisions |
| `references-many` | over 12 pages: merge pages |
| `orphan-reference` | route it from `SKILL.md` (one level deep), or delete it |
| `lobby-reference-unlisted` / `lobby-script-unlisted` | add one lobby route, no second catalog; helpers may sit behind a routed command |
| `lobby-workflow-missing` | a `Flow:` line or `## Workflow` heading on its own line |
| `lobby-map-missing` / `lobby-map-incomplete` | draw the skill map; it covers every page in the references, docs, and scripts/docs folders |
| `script-unreferenced` | import it, name it, or drop it |
| `route-condition` | state when or why on the same line as the route |
| `reference-entry-cue` / `reference-why-cue` | open with `Load when … Why: …` |
| `reference-dead-end` | add the next hop, or say the step ends here |
| `flow-phase-unrouted` | name each flow phase in a route or gate, or drop it |

Navigation WARNs are candidates, not mandatory format; audit trails, templates, and fixtures skip entry and exit cues. A routed directory (including `scripts/`) covers its files, but a literal missing file in it still fails. Write example paths with a placeholder (`scripts/<hook-directory>/`).

## Judgment checks the script cannot make

Check that every lobby ACT rule holds, including `## Output`, and that the prose has no filler and no lost data. The script does not check that section. Also check that each routed file owns a job that changes the next action, and that a declared host scheme is exposed. Reachability does not prove a routed adapter is useful; verify runtime selection separately.

Next: to fix findings load `references/skill-improve.md`; for design guidance load `references/skill-anatomy.md`.
