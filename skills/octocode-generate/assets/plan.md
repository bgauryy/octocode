---
status: Draft
project: <name>
target: <absolute folder>
context: <workspace>/.octocode/octocode-generate/<slug>/context/
design: <workspace>/.octocode/octocode-generate/<slug>/design.md
---

# Generation plan: <name>

## Requirements

| Topic | Answer | Source |
|---|---|---|
| Purpose and core features | | user |
| Runs on / rendering | | user |
| Package manager and runtime | | detected / user / assumed |
| Deployment target | | |
| Out of scope | | |
| Preset | | user / assumed |

## Context

Mocks, designs, specs, sample data and reference apps from the user, saved under `context/`.

| Item | What it decides (screens, fields, flows, style) |
|---|---|

## Decisions

One row per choice that is costly to reverse (framework, runtime, package manager, lint lane, database, auth).

| Decision | Chosen | Runner-up and why it lost | Evidence | Revisit if |
|---|---|---|---|---|
| Framework | | | | |

## Evidence

Every command, flag, default and version comes from a lookup made in this run (the skill's `references/research.md`).

| Claim | Source (`owner/repo@commitSha:path:line`, URL or tool result) |
|---|---|
| Scaffold command and what it sets up | |
| Template files kept or added | |
| CI action pins | |
| Advisories checked (`securityCheck`) | |

## Integrations

| Service | SDK | Env vars | Tested with (fake, emulator, local server) | Docs |
|---|---|---|---|---|

## Tests

Minimal: one test per core feature, one smoke test of the main flow, end to end only for critical paths.

| Feature | Test |
|---|---|

## Scaffolder extras

Agent files, telemetry, git init or deploy steps the scaffolder adds, and the flag that turns each off.

| Extra | Keep or opt out (flag) |
|---|---|

## Versions

Summary of `check-versions.mjs <every planned package>`: chosen versions, conflicts and their fixes, missing peers, deprecations.

| Package | Version | Note |
|---|---|---|

## Risks

The likeliest ways this scaffold fails (peer conflicts, a missing toolchain, runtime or adapter limits, licensing, missing env) and the check that catches each.

## Steps

1. [ ] Create: `<creational CLI or boilerplate command with non-interactive flags>` in `<target>` (see the skill's `references/boilerplates.md`)
2. [ ] Templates: `apply-template.mjs <target> base <templates for the stack> dependabot --pm <pm>` (dry run, then `--write`)
3. [ ] Install: `<install commands from apply-template and the Versions table>`
4. [ ] Implement: <feature> → <files> (one line per feature, each with its first test)
5. [ ] Format once, then verify: `verify-project.mjs <target>` (plus `--gate` rows for custom gates)
6. [ ] Hand over: README run section and `AGENTS.md` with the dev and gate commands

## Quality gates

| Gate | Command | Required |
|---|---|---|
| format | | yes |
| lint | | yes |
| typecheck | | when typed |
| test | | yes |
| build | | yes |
| dev server | `<dev command>` starts without errors | apps |

## Approval

Asked through the question tool after `design.md` and this plan were written. Building starts only after approval.

Approved by the user on <date>, with changes: <none or list>. Set `status: Approved` here and in `design.md`; set `Built` after the gates pass.
