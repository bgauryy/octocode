---
status: Draft
project: <name>
target: <absolute folder>
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

## Decisions

One row per choice that is costly to reverse (framework, runtime, package manager, lint lane, database, auth).

| Decision | Chosen | Rejected and why | Evidence (2+ sources for framework, auth, data) | Revisit if |
|---|---|---|---|---|
| Framework | | | | |

## Versions

Paste the summary of `check-versions.mjs <every planned package>`: chosen versions, conflicts and their fixes, missing peers, deprecations.

| Package | Version | Note |
|---|---|---|

## Risks

Pre-mortem: the three likeliest ways this scaffold fails (peer conflicts, runtime or adapter limits, licensing, missing env) and the check that catches each.

## Steps

1. [ ] Scaffold: `<framework CLI with non-interactive flags>` in `<target>`
2. [ ] Templates: `apply-template.mjs <target> base <stack> <lint lane> --pm <pm>` (dry run, then `--write`)
3. [ ] Install: `<install commands from apply-template and the Versions table>`
4. [ ] Implement: <feature> → <files> (one line per feature, each with its first test)
5. [ ] Format once, then verify: `verify-project.mjs <target>`

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

Approved by the user on <date>, with changes: <none or list>.
