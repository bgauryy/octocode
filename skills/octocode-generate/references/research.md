# Research with Octocode and the web

Load before writing the plan, and when a scaffold, install or gate fails. The catalogs name each row's docs as `owner/repo/path` or a URL (rows verified on 2026-11-26). Re-read that source at the current release before trusting a flag, then cite the finding in the plan's Evidence table as `owner/repo@commitSha:path:line` or a URL. Save longer notes to the project's `context/research.md`.

A preset the user accepted needs steps 1–3 and 6–7. A new SDK or an unusual feature adds steps 4–5, a jump from a version you know adds step 8, and a failure uses [the failure table](#when-something-fails).

| Mode | Use for | How |
|---|---|---|
| GitHub (Octocode) | Scaffolder commands and templates, official examples, SDK usage in real code, changelogs, CI action pins | The recipe below |
| Web | Docs that live only on a website, provider comparisons, pricing and limits, announcements newer than a release | The host's web search; read a page with `clasify` `source:{"url":…}` and its `next.read`, or the host's fetch tool |
| Brainstorm | An open idea, or several viable stacks with no deciding constraint | The `octocode-brainstorming` skill when installed: options, then evidence for and against each |

Use the Octocode MCP tools when they are listed. Otherwise the CLI takes the same input: `npx -y octocode <tool> '{"queries":[…]}'`. GitHub tools need a sign-in; `npx -y octocode auth status` shows it. Batch independent lookups (up to 5 queries in one call). Follow every reply's `next` before citing.

## Recipe

| Step | Call | Keep for the plan |
|---|---|---|
| 1. Resolve the scaffolder and framework | `artifactSearch` `{"operation":"resolve","ecosystem":"npm","packageName":"create-next-app"}` (ecosystems: npm, pypi, crates, maven, nuget, go, packagist, rubygems) | Version, source `owner/repo`, release `ref` |
| 2. Read the current command | `ghGetFileContent` on the row's docs path at that `ref`, with `matchString` on the command name; for exact flags, the CLI's argument parser in the same repository. A doc that lives only on a website: the Web mode above | The non-interactive command, what it sets up, any prompt it still shows |
| 3. Read the generated template | `ghStructure` `tree` of the template folder at the `ref`, then `ghGetFileContent` on its manifest and config files | Scripts, lint and test tooling, files the skill's templates must not duplicate |
| 4. Find the closest official example | `ghStructure` `tree` of the framework's `examples/` folder, or `ghSearchCode` with the framework's `owner` and a feature term (`betterAuth`, `drizzle`, `expo-router`) | Integration code to follow, at the version you install |
| 5. Community reference apps, only for a feature no official example covers | `ghSearchRepo` with `keywords`, `filters.pushedAt.from` (recent), `sort:{"by":"stars"}`; then `ghStructure` on the best hit | A maintained app on the same major versions |
| 6. Dependency trust | After `check-versions.mjs`: `securityCheck` `{"operation":"package","ecosystem":"npm","packageName":"<name>","version":"<resolved>"}` for each runtime dependency (ecosystems npm, pypi, crates, maven, nuget, go, packagist, rubygems, Pub, Hex, SwiftURL; `GitHub Actions` for workflow actions) | Advisories and `upgradeTo`. `affected:false` means none known to OSV, not proven safe |
| 7. CI action pins | `ghStructure` `{"operation":"refs","owner":"actions","repo":"checkout","ref":"v7"}` for each `uses:` in workflows the scaffolder generated | `uses: owner/repo@<commitSha> # <tag>`. A tag that does not resolve fails CI: `astral-sh/setup-uv` has no `v10` tag |
| 8. Breaking changes since a version you know | `ghGetHistoryItem` compare with `baseRef` and `headRef`, or the CHANGELOG at the `ref` | Migrations the plan must apply |

`ghSearchCode` searches the default branch only and needs an `owner`. Re-read a hit at the release `ref` you install.

## Judge sources

Prefer, in order: the framework's docs and template at the release you install, its official examples, then maintained apps on the same major versions. Drop patterns from another major version, archived repositories and forks. When two sources disagree, the one at the installed release wins; note the other in the plan. Your own memory of a framework is a lead to check, never a source.

## When something fails

| Failure | Research |
|---|---|
| A flag is rejected or a prompt appears | Step 2 on the CLI's argument parser at the installed version; rerun with the documented flag |
| Peer or version conflict | `check-versions.mjs` `fixPeer` and `fixPackage`; the peer's CHANGELOG at that version |
| Config or type error after install | The template's config at the installed `ref` (step 3); `ghSearchCode` the error text in the library's repository |
| A gate fails on code the scaffolder generated | Fix the code. A template that ships failing (as `swift package init` does for `swift format lint`) needs the formatter run once: record that in the plan |
| A tool is `not-installed` | Install it from its official docs (step 2) when the plan allows; otherwise report the gate as blocked |
| Nothing found | Try other spellings, then the library's issues (`ghSearchHistory`), then ask the user |
