# Skill installer architecture

`@octocodeai/octocode-skill-installer` is the single filesystem and platform
contract used by the Octocode and Awareness CLIs. Bundling remains owned by each
caller: Octocode supplies its complete skill suite, while Awareness supplies only
`octocode-awareness`.

## Data flow

```text
package bundle ──atomic copy──> $OCTOCODE_HOME/skills/<name>
                                      │
                                      ├──directory symlink──> global host directory
                                      └──directory symlink──> project host directory
```

The canonical copy is durable across npm cache eviction and package upgrades.
Platform links never point into `node_modules`, an `npx` cache, or package build
output. Windows uses directory junctions for link mode.

## Ownership

- `SKILL_PLATFORMS` owns canonical platform names, aliases, supported scopes,
  destination paths, and `auto` compatibility choices.
- `installBundledSkills` owns validation, dry-run planning, atomic replacement,
  conflict refusal, idempotency, and the shared result schema.
- Calling CLIs own argument parsing, bundled-skill discovery, human output, and
  any package-specific follow-up guidance.
- `@octocodeai/config` owns `$OCTOCODE_HOME` resolution.

## Safety invariants

- A bundled skill must contain a regular, non-symlink `SKILL.md`.
- The installer leaves existing canonical or destination content unchanged unless
  `force` is true.
- Canonical materialization and destination replacement stage beside the target,
  rename the old target to a backup, publish the replacement, then remove the
  backup. A failed publish restores the backup.
- Directory links target the durable canonical directory, never the transient
  package source.
- Repeating an identical request returns an unchanged status.
- `dryRun` executes validation and conflict detection without filesystem writes.

## Platform contract

The public platform names are `pi`, `cursor`, `claude`, `claude-desktop`,
`codex`, `codex-native`, `opencode`, `copilot`, and `gemini`. `shared`, `common`,
and `agents` normalize to `codex`; `all` expands to every platform. All platforms
support global scope. Claude Desktop does not support project scope.
