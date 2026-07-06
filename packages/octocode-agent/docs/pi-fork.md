# Pi Fork Dev Guide

`octocode-agent` bundles `@earendil-works/pi-coding-agent` as the Pi host. This guide
shows how to run a locally-built fork of Pi instead — no code change required.

## Why Fork

The Pi extension API (`@octocodeai/pi-extension`) controls tools, system prompt, skills,
and events. Forking Pi gives additional control over:

- Default CLI flags and startup behaviour
- TUI branding and layout changes
- Session / compaction internals
- Core slash commands
- Own publish cadence (no dependency on upstream releases)

## Fork Setup (one-time)

### 1. Fork on GitHub

Go to https://github.com/earendil-works/pi → **Fork** → your account or org.

```bash
# Clone your fork next to the octocode monorepo
git clone https://github.com/<your-org>/pi.git ~/code/pi-fork
cd ~/code/pi-fork

# Track upstream for sync
git remote add upstream https://github.com/earendil-works/pi.git
git fetch upstream
```

### 2. Rename the package (optional but recommended for prod)

In `packages/coding-agent/package.json`:
```json
{
  "name": "@octocodeai/pi-coding-agent",
  "version": "0.80.3-octocode.0"
}
```

Keep `"bin": { "pi": "..." }` unchanged — the launcher resolves it from the `bin` field.

### 3. Build the fork

```bash
cd ~/code/pi-fork
npm install
npm run build          # or: npx tsc -p packages/coding-agent/tsconfig.build.json
```

The built entry is typically at `packages/coding-agent/out/cli.js`.

## Local Dev Workflow

Point `octocode-agent` at the fork binary with an env var — **no package.json edit**:

```bash
# Find the pi binary in the fork
ls ~/code/pi-fork/packages/coding-agent/out/  # look for cli.js or similar

export OCTOCODE_PI_BIN=~/code/pi-fork/packages/coding-agent/out/cli.js
octocode-agent          # uses the fork binary
octocode-agent --version  # shows: pi host ... (local binary: ...)
```

Add to `~/.octocode/.env` or a `.env` in the project to persist across sessions.

## Production Fork Workflow

When the fork is published to npm (e.g. `@octocodeai/pi-coding-agent`):

```bash
# 1. Update octocode-agent/package.json dependency
#    "@earendil-works/pi-coding-agent" → "@octocodeai/pi-coding-agent"

# 2. Update PI_PACKAGE constant in bin/launcher.mjs

# 3. Or — override without changing code:
export OCTOCODE_PI_PACKAGE=@octocodeai/pi-coding-agent
```

## Syncing with Upstream

```bash
cd ~/code/pi-fork
git fetch upstream
git merge upstream/main --no-ff -m "chore: sync upstream earendil-works/pi@$(git rev-parse --short upstream/main)"
# Resolve conflicts; focus on packages/coding-agent/
npm run check && ./test.sh   # fork's CI gate
```

## Env Var Reference

| Env var | Effect |
|---|---|
| `OCTOCODE_PI_BIN` | Absolute path to a locally-built Pi binary (takes priority) |
| `OCTOCODE_PI_PACKAGE` | npm package name override (e.g. `@octocodeai/pi-coding-agent`) |
| `OCTOCODE_AGENT_CONTEXT_FILES=1` | Re-enable `AGENTS.md`/`CLAUDE.md` loading (off by default) |
| `OCTOCODE_AGENT_EXTENSION_SPEC` | Override the core extension spec (npm:/git:/path) |
| `OCTOCODE_AGENT_CLEAN=1` | Suppress user skills (fully deterministic agent) |
| `OCTOCODE_AGENT_FULL_TOOLS=1` | Keep grep/find/ls (opt out of lean tool set) |

## What `--no-context-files` Does (default since Phase 0)

`octocode-agent` now passes `--no-context-files` to Pi by default. This means:

- `AGENTS.md` and `CLAUDE.md` are **never loaded** — Pi skips discovery entirely
- Saves tokens; avoids conflicts with the Octocode structured system prompt
- The `@octocodeai/pi-extension` already cleared `contextFiles` in `before_agent_start`
  as a belt-and-suspenders measure; this is now redundant but kept as defense-in-depth

To re-enable (e.g. for a project that relies on `AGENTS.md`):
```bash
OCTOCODE_AGENT_CONTEXT_FILES=1 octocode-agent
```
