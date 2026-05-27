# Plan: Remove GitLab & Bitbucket from Octocode MCP

**Status:** Draft
**Owner:** @bgauryy
**Date:** 2026-05-26

## Goal

Completely remove GitLab and Bitbucket support — code, tests, configuration, dependencies, and documentation — leaving GitHub as the only supported code-host provider. The `ICodeHostProvider` abstraction stays (still useful for testability and a possible future provider), but the `ProviderType` union collapses to `'github'`.

## Scope summary

- **131 files** reference `gitlab` / `bitbucket` (case-insensitive) across the repo.
- Source: 2 top-level dirs (`src/gitlab/`, `src/bitbucket/`) + 2 provider-adapter dirs (`src/providers/gitlab/`, `src/providers/bitbucket/`) + 2 config modules + scattered references in routing/factory/types/hints.
- Tests: 2 top-level dirs (`tests/gitlab/`, `tests/bitbucket/`) + 2 provider-adapter dirs + branch coverage tests across `tests/providers/` and `tests/tools/`.
- Shared config (`packages/octocode-shared/src/config/`): types, defaults, resolver sections, validator, schema all carry gitlab/bitbucket fields.
- Docs: 4 provider-setup guides, 2 reference docs, AGENTS.md, READMEs, troubleshooting, architecture, spec.
- One npm dep: `@coderabbitai/bitbucket` in `packages/octocode-mcp/package.json`.

## Non-goals

- Keeping a feature flag or compat shim for future re-enable. Pure deletion.
- Touching the public MCP tool surface beyond removing provider-routing/hints — tool names, schemas, and inputs stay GitHub-shaped (they already are).
- Migrating user config files automatically — a release-note callout instead.

## Phase 1 — Code: provider routing and abstraction

Cut routing first so nothing imports the modules we delete next.

### 1.1 `src/providers/` core wiring
| File | Action |
|---|---|
| `src/providers/types.ts` | `ProviderType` → `'github'` only. Drop `gitlab`/`bitbucket` doc strings in `ProviderConfig` JSDoc. |
| `src/providers/factory.ts` | Remove both `tryInitProvider('gitlab', …)` and `tryInitProvider('bitbucket', …)` blocks in `initializeProviders()` (lines 294–303). Update `getProvider` JSDoc example. |
| `src/providers/capabilities.ts` | Drop `gitlab` and `bitbucket` entries from `PROVIDER_CAPABILITIES`. Reconsider whether `ProviderCapabilities` is still needed (only one consumer left) — keep for now, simplify later. |
| `src/providers/providerQueries.ts` | Remove `gitlab`/`bitbucket` mentions in `ProviderType` re-export and the comments referencing GitLab project IDs / MR iids. |
| `src/providers/providerResults.ts`, `contentExtraction.ts`, `pullRequestFileChanges.ts` | Strip any GitLab/Bitbucket-specific branches if present. |

### 1.2 Delete adapter dirs
- `rm -rf packages/octocode-mcp/src/providers/gitlab/`
- `rm -rf packages/octocode-mcp/src/providers/bitbucket/`

### 1.3 Delete REST/SDK client dirs
- `rm -rf packages/octocode-mcp/src/gitlab/` (10 files)
- `rm -rf packages/octocode-mcp/src/bitbucket/` (10 files)

### 1.4 Delete config modules
- `rm packages/octocode-mcp/src/gitlabConfig.ts`
- `rm packages/octocode-mcp/src/bitbucketConfig.ts`

### 1.5 `src/serverConfig.ts`
- Remove imports from `./gitlabConfig.js` and `./bitbucketConfig.js` (lines 15–25).
- Remove `gitlab: resolveGitLabConfig()` / `bitbucket: resolveBitbucketConfig()` from whatever config object aggregates them (line 147–148).
- `getActiveProvider()`: collapse to `return 'github'` (or just remove the function if it has no remaining callers; verify with `rg getActiveProvider`).
- Remove `isGitLabActive()` (unused outside this file per grep).
- Remove the GitLab/Bitbucket branches in the provider-config returning function (lines 244–256).

### 1.6 `src/types.ts`
- Delete `GitLabTokenSourceType`, `GitLabConfig`, `BitbucketTokenSourceType`, `BitbucketConfig` (lines 272–313).
- Remove `gitlab?` and `bitbucket?` fields from whatever aggregate interface contains them (lines 333–336).

### 1.7 Tool-level hint cleanup
- `src/tools/github_search_code/hints.ts` (lines 130–137): replace the `tokenVarMap` lookup with a hard-coded `'GITHUB_TOKEN'`. Drop `getActiveProvider()` import if it becomes unused.
- `src/tools/package_search/execution.ts` (line 48): narrow the regex `/(?:github\.com|gitlab\.com|bitbucket\.org)\/…/` to `/github\.com\/…/`. Verify downstream parsing still expects only one host.
- `src/utils/package/python.ts` and `src/utils/exec/spawn.ts`: re-grep — likely just URL strings or comments. Trim references.

### 1.8 Package manifest
- `packages/octocode-mcp/package.json`: remove `"@coderabbitai/bitbucket": "^1.1.4"`. Re-run `npm install` (or workspace equivalent) and commit the lockfile update.

## Phase 2 — Code: shared config package

Order matters: the MCP package must stop reading these fields before the shared package drops them.

| File | Action |
|---|---|
| `packages/octocode-shared/src/config/types.ts` | Delete `GitLabConfigOptions`, `BitbucketConfigOptions`, `RequiredGitLabConfig`, `RequiredBitbucketConfig`. Remove the `gitlab?` / `bitbucket?` fields from `OctocodeConfig` and the `gitlab` / `bitbucket` fields from `ResolvedOctocodeConfig`. |
| `packages/octocode-shared/src/config/defaults.ts` | Delete `DEFAULT_GITLAB_CONFIG`, `DEFAULT_BITBUCKET_CONFIG`, and their entries in the aggregate default (lines 22–32, 93–94). |
| `packages/octocode-shared/src/config/resolverSections.ts` | Delete `resolveGitLab`, `resolveBitbucket` (lines 120–138) and their type imports. |
| `packages/octocode-shared/src/config/resolverCache.ts` | Remove `resolveGitLab` / `resolveBitbucket` imports (lines 16–17) and the assignments in the resolved object (lines 71–72). |
| `packages/octocode-shared/src/config/validator.ts` | Remove any per-section validation for gitlab/bitbucket. |
| `packages/octocode-shared/src/config/schemas.ts`, `index.ts` | Drop re-exports / schema entries referencing the removed types. |

## Phase 3 — Tests

### 3.1 Delete test dirs entirely
- `rm -rf packages/octocode-mcp/tests/gitlab/` (7 files)
- `rm -rf packages/octocode-mcp/tests/bitbucket/` (10 files)
- `rm -rf packages/octocode-mcp/tests/providers/gitlab/` (3 files)
- `rm -rf packages/octocode-mcp/tests/providers/bitbucket/` (3 files)
- `rm packages/octocode-mcp/tests/bitbucketConfig.test.ts`

### 3.2 Update mixed-provider tests
The grep surfaced these tests as referencing both providers; each needs targeted edits, not deletion:

| Test file | Likely change |
|---|---|
| `tests/providers/factory.test.ts`, `factory.branches.test.ts`, `factory.diagnostics.test.ts` | Drop assertions that gitlab/bitbucket register; assert only github registers; remove unknown-provider tests that used gitlab as the known case. |
| `tests/providers/types.test.ts` | Update `ProviderType` exhaustiveness checks. |
| `tests/providers/rateLimit.integration.test.ts` | Strip non-github cases. |
| `tests/tools/providerExecution.test.ts` | Same. |
| `tests/tools/remote-tools.contract.test.ts` | Remove non-github contract rows. |
| `tests/tools/github_search_code.tool.test.ts` | Drop the 401 hint tests that exercise `GITLAB_TOKEN` / `BITBUCKET_TOKEN` branches. |
| `tests/tools/package_search.test.ts`, `package_search.execution.branches.test.ts` | Remove gitlab.com / bitbucket.org URL parsing cases. |
| `tests/tools/github_clone_repo.test.ts`, `github_fetch_content.directory.test.ts` | Trim incidental references. |
| `tests/serverConfig.initRecovery.test.ts`, `tests/index.test.ts`, `tests/session.branches.test.ts` | Remove gitlab/bitbucket env-var setups and assertions. |
| `tests/flows/remote.search-to-fetch-content.flow.test.ts`, `tests/flows/catalog.ts` | Strip non-github flow rows. |
| `tests/setup.ts` | Drop mocks for `octocode-shared` gitlab/bitbucket exports if any. |
| `tests/utils/pythonPackage.test.ts` | Likely just URL fixtures — narrow to github. |
| `packages/octocode-shared/tests/config/validator.test.ts`, `resolver.test.ts`, `session/storage.test.ts` | Remove gitlab/bitbucket sections. |

### 3.3 octocode-cli sibling package
- `packages/octocode-cli/src/cli/tool-command.ts` and its two tests already appear in `git status` — verify they don't add new gitlab/bitbucket refs. If they do, strip during this phase.

## Phase 4 — Documentation

### 4.1 Delete
- `docs/configuration/providers/GITLAB_SETUP_GUIDE.md`
- `docs/configuration/providers/BITBUCKET_SETUP_GUIDE.md`
- `docs/dev/reference/GITHUB_GITLAB_TOOLS_REFERENCE.md` → rename to `GITHUB_TOOLS_REFERENCE.md`, strip GitLab columns/sections. (Keep file; rename to preserve relative-link history if other docs point at it — see 4.3.)

### 4.2 Edit
- `README.md`, `packages/octocode-mcp/README.md`, `docs/README.md`, `docs/dev/README.md`
- `docs/configuration/README.md`, `docs/configuration/CONFIGURATION_REFERENCE.md`, `docs/configuration/TROUBLESHOOTING.md`
- `docs/configuration/providers/AUTHENTICATION_SETUP.md`, `docs/configuration/providers/GITHUB_SETUP_GUIDE.md`
- `docs/configuration/clients/PI_SETUP_GUIDE.md`
- `docs/dev/DEVELOPMENT_GUIDE.md`, `docs/dev/architecture/SESSION_PERSISTENCE.md`, `docs/dev/workflows/CLONE_AND_LOCAL_TOOLS_WORKFLOW.md`
- `docs/dev/reference/LOCAL_TOOLS_REFERENCE.md`, `docs/dev/reference/SHARED_API_REFERENCE.md`
- `docs/specs/2026-04-16-octocode-agent-cli-design.md` — leave the historical spec but add a one-line "superseded by [this plan]" note rather than rewriting.
- `AGENTS.md`, `DCOC.md`
- `skills/octocode-pull-request-reviewer/README.md`, `skills/octocode-research/scripts/server.js`
- `packages/octocode-mcp/sanity_tests/11_githubCloneRepo.md`

For each: delete provider-list bullets, env-var rows (`GITLAB_TOKEN`, `BITBUCKET_TOKEN`), config-snippet sections, "supported providers" tables, and links to deleted guides.

### 4.3 Link integrity
After 4.1/4.2, run `rg '(GITLAB_SETUP_GUIDE|BITBUCKET_SETUP_GUIDE|GITHUB_GITLAB_TOOLS_REFERENCE)' docs/ README.md AGENTS.md` and fix any dangling references.

### 4.4 CLAUDE.md / memory
- `CLAUDE.md` references AGENTS.md only — no change.
- Update the user's auto-memory note at `~/.claude/projects/.../memory/MEMORY.md`: the line "Providers: GitHub, GitLab, Bitbucket — factory pattern in `src/providers/factory.ts`" needs to become "Providers: GitHub only (GitLab/Bitbucket removed YYYY-MM-DD)". Do this as part of the PR so the next session has accurate context.

## Phase 5 — Verification

Run in order; treat each as a gate.

1. `pnpm -w typecheck` (or `tsc --noEmit` at each package) — first signal that dangling imports/types are gone.
2. `pnpm -w lint`.
3. `pnpm --filter octocode-mcp test`, `pnpm --filter octocode-shared test`, `pnpm --filter octocode-cli test`.
4. `rg -i '\b(gitlab|bitbucket)\b' --type-not lock` from repo root — expect **zero** hits. Anything left is either a missed reference or an intentional historical mention (spec note, changelog) that needs an explicit allowlist comment.
5. Boot the MCP server (`node dist/index.js` or the existing launch script) and confirm `initializeProviders()` logs only one provider, no warnings.
6. Smoke-test one tool end-to-end (e.g. `mcp__octocode-local__githubSearchCode`).
7. Inspect `packages/octocode-mcp/package.json` and the lockfile for any orphaned transitive deps from `@coderabbitai/bitbucket`.

## Phase 6 — Release

- Bump `packages/octocode-mcp` to a major (breaking removal of provider config keys).
- Bump `packages/octocode-shared` to a major (breaking type removal).
- CHANGELOG entry under "Removed": list env vars (`GITLAB_TOKEN`, `GITLAB_HOST`, `BITBUCKET_TOKEN`, `BITBUCKET_HOST`, `BITBUCKET_USERNAME`) and config keys (`gitlab`, `bitbucket`) that will be silently ignored after upgrade.
- Migration note: users who set those env vars in `mcp.json` should remove them — they no longer route anywhere and may surface as unknown-key warnings if config validation is strict.

## Execution order recap

1. Phase 1 (MCP source) — breaks only when shared types still expect fields; that's fine because the MCP package no longer reads them.
2. Phase 2 (shared package) — now safe to drop the fields.
3. Phase 3 (tests) — must come after 1 and 2 or test files reference deleted symbols.
4. Phase 4 (docs) — independent; can run in parallel with 3.
5. Phase 5 (verify) — gate.
6. Phase 6 (release) — last.

## Risks

- **External consumers of `octocode-shared`** that destructure `gitlab` / `bitbucket` from the resolved config will break at the type level on upgrade. Major-version bump + CHANGELOG covers this.
- **`getActiveProvider()` removal** — grep showed only one same-file reference, but worth a second pass after Phase 1.1 — there may be dynamic string lookups that grep misses.
- **`@coderabbitai/bitbucket` transitive deps** could leave orphaned packages until the lockfile is regenerated; Phase 5 step 7 catches that.
- **Spec doc at `docs/specs/2026-04-16-octocode-agent-cli-design.md`** was the architecture intent for multi-provider. Decide whether to rewrite or just mark superseded — recommend the latter (history is useful context, rewriting destroys it).

## Open questions

1. Keep the `ICodeHostProvider` interface and `ProviderType` alias for forward-compatibility, or inline GitHub calls and delete the abstraction entirely? **Recommend keep** — tests already depend on it, and reintroducing it later costs more than the few KB of code.
2. Does any downstream Wix tooling read `GITLAB_TOKEN` / `BITBUCKET_TOKEN` from the same env it shares with octocode? Worth a quick `rg` of sibling repos before the major bump.
