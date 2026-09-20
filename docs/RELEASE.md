# Release checklist

Publishing is owner-triggered and never runs from CI or an agent session.
Every gate below is machine-checked; a release that skips one reproduces a
silent-state incident from the 2026-09 audits.

## Order of operations

Core publishes **before** the packages that embed its contracts. The native
binary compiles the core contract IR at `contracts:regen` time, so a stale
published core makes a clean `npm install` diverge from the shipped binary.

1. **Core** (`octocode-mcp-host` repo): publish `@octocodeai/octocode-core`
   at the version this workspace pins in the root `package.json`.
2. **Native** (`@octocodeai/octocode-native`), then **CLI** (`octocode`).

## Gates (all must be green)

| Gate | Command | What it proves |
|---|---|---|
| Full verify | `yarn workspace @octocodeai/octocode-native verify` | fmt, clippy, tests, loader/ABI/version checks, contract pin, doc claims |
| Contract sync (dev) | `yarn workspace @octocodeai/octocode-native contracts:check` | Embedded contracts match the resolved core checkout; regen source was a **clean** core commit (`sourceDirty: false`) |
| Contract sync (published) | `yarn workspace @octocodeai/octocode-native contracts:check:published` | Embedded contracts match the **npm-published** core at the pinned version — what a clean install actually delivers. Runs automatically in `prepublishOnly`; blocks publish only, never dev |
| Docs drift | `yarn workspace @octocodeai/octocode-native docs:claims` | README exit codes, tool count, and documented env names match source |
| Launcher e2e | CI `launcher-e2e` job (Linux) | The built npm launcher drives the real native binary end-to-end |
| Version consistency | `yarn workspace @octocodeai/octocode-native version:check` | Workspace versions and platform sub-packages agree |

## Failure playbook

- `contract-sync: … DIRTY core checkout` — the embedded contracts were
  regenerated against uncommitted core changes. Commit (or reset) the core
  checkout, run `yarn contracts:regen`, and re-verify.
- `contract-sync: FINGERPRINT MISMATCH` in `--published` mode — publish the
  matching core first (step 1), or align the pinned core version; never
  publish native around this gate.
- Docs-drift failures — fix the doc or the source. Shrink the pinned claim
  set if a pin false-positives twice in a month; never drop the gate
  (RFC post-audit-hardening-2026-09, R3).
