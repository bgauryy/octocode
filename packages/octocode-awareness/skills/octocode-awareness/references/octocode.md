# Octocode Search (via `npx octocode`)

Use this when awareness work needs code, GitHub, package, history, artifact, or graph evidence before locking, signaling, recording memory, or reflecting.

**Do not** expect a bundled `octocode` binary inside this skill. The native engine is platform-installed; agents must call the published CLI:

```bash
npx octocode <command> ... --no-color
```

Prefer MCP `user-octocode-local` tools when already connected in the host. Fall back to `npx octocode` when MCP is unavailable. For deep research workflows, also load `octocode-research` if installed.

## Why `npx octocode` (not a vendored copy)

- `@octocodeai/octocode-engine` ships platform-specific native addons.
- `npx octocode` resolves the correct engine for the host OS/arch.
- Copying `packages/octocode/out` into awareness would break offline/wrong-platform installs.

## Agent recipes (copy-runnable)

```bash
# Orient
npx octocode search <dir> --tree --max-depth 2 --no-color

# Local text / symbols
npx octocode search "<term>" <path> --no-color
npx octocode search <file> --content-view symbols --no-color
npx octocode search <file> --content-view exact --no-color
npx octocode search <file> --op references --symbol <Name> --line <N> --no-color

# Find files
npx octocode search <dir> --search path --name "<glob>" --no-color

# External
npx octocode search <keywords> --target repositories --no-color
npx octocode search <pkg> --target packages --no-color
npx octocode search owner/repo#N --target pullRequests --no-color
npx octocode search owner/repo/path --target commits --no-color

# Schema before raw OQL
npx octocode search --scheme --compact --no-color
```

Treat search hits as leads. Cite paths/lines/IDs in locks, signals, memories, and refinements. Zero matches ≠ absence — change scope, mode, or spelling before concluding.

## Skill install (separate from search)

```bash
npx octocode skill --name octocode-research
npx octocode skill --add --path "{{path_to_skills_location}}/octocode-awareness" --platform common
```

Add `--platform <target>` (`codex`, `claude`, `cursor`, `pi`) when installing skills for a specific host.

## Boundary

Awareness owns coordination (attend, locks, signals, verify, reflect, wiki projections).
Octocode owns research/search. Return evidence here only to inform awareness actions.
