# AGENTS.md — `packages/octocode-skills`

Agent guide for `@octocodeai/skills`. Read this before any edit to this package.
Root `AGENTS.md` applies everywhere; this file narrows scope to this package only.

---

## What this package is

A zero-runtime-dep CLI + library that:
1. **Bundles** all skills from `../../skills/` at build time into `skills/` (package root).
2. **Lists** bundled skills with install status and env readiness.
3. **Installs** skills to a canonical home (`~/.octocode/skills/<name>/`) then symlinks into platform dirs.
4. **Checks** all installation locations and env param configuration.
5. **Informs** — every command tells the user what is missing and how to fix it.

**Hard invariants:**
- Zero npm runtime dependencies — Node.js builtins only (`node:fs`, `node:path`, `node:os`, `node:url`).
- `src/cli.ts` must NOT start with `#!/usr/bin/env node` — esbuild adds it via `banner`; a source shebang creates a double-shebang that crashes Node ESM.
- All TypeScript is compiled with `exactOptionalPropertyTypes: true` — never assign `undefined` to an optional property; use conditional spread `...(x !== undefined ? { x } : {})`.
- `out/` is always generated — never edit it by hand; always rebuild after changes.

---

## Directory map

```
packages/octocode-skills/
├── src/
│   ├── cli.ts              ← CLI entry point — arg parsing, command dispatch, help text
│   ├── index.ts            ← Library exports (programmatic API)
│   ├── registry.ts         ← Read bundled skills list; parse SKILL.md frontmatter
│   ├── installer.ts        ← Copy/symlink a skill to home, platform dirs, workspace, custom path
│   ├── checker.ts          ← Probe install locations: installed | linked | broken | missing
│   ├── env-params.ts       ← Static env param registry per skill; runtime set/missing check
│   ├── home.ts             ← getOctocodeHome(), getSkillsHome() — inlined, zero deps
│   ├── platforms.ts        ← Platform → dir mapping; parsePlatforms()
│   ├── commands/
│   │   ├── list.ts         ← `octocode-skills list`    — skills + install + env status
│   │   ├── install.ts      ← `octocode-skills install` — install with override/keep + env warning
│   │   ├── check.ts        ← `octocode-skills check`   — verify install + env per skill
│   │   └── info.ts         ← `octocode-skills info`    — SKILL.md + env params detail
│   └── utils/
│       ├── colors.ts       ← dim / bold / green / yellow / red / cyan — single-file, no dep
│       └── spinner.ts      ← TTY spinner on stderr; silent when !isTTY or CI=true
├── skills/                 ← GENERATED at build — copy of ../../skills/ (no scripts/ dirs)
├── out/                    ← GENERATED — esbuild bundles (cli.js, index.js)
├── build.mjs               ← Build script: clean → sync skills → esbuild CLI + index
├── package.json
├── tsconfig.json
└── README.md
```

---

## Data flow

```
../../skills/<name>/SKILL.md   ← source of truth for skill content
        ↓  build.mjs (sync at build time, excludes scripts/)
skills/<name>/SKILL.md         ← bundled copy inside this package
        ↓  src/registry.ts
listSkills() / getSkill()      ← SkillInfo { name, folder, description, dir }
        ↓  src/env-params.ts
getSkillEnvStatus()            ← SkillEnvStatus { readiness, params[] }
        ↓  src/checker.ts
checkSkill()                   ← SkillCheckResult { home, platforms[], workspace }
        ↓  src/commands/*.ts
list / install / check / info  ← human or JSON output to stdout
```

All four commands read from these three modules. Changes to skill content flow through
registry; changes to env requirements flow through env-params; installation state flows
through checker.

---

## Task: Adding a new skill

**1. Create the skill folder** in `../../skills/<new-name>/` with at least a `SKILL.md`
   that has YAML frontmatter:
   ```yaml
   ---
   name: <new-name>
   description: "One sentence — shown in list and info."
   ---
   ```

**2. Register env params** in `src/env-params.ts` → `SKILL_ENV_PARAMS`:
   ```ts
   // If the skill needs no env params — do nothing. Skills absent from the map are treated as "ok".

   // If it needs web search (like brainstorming):
   'new-skill-name': WEB_SEARCH_PARAMS,

   // If it needs GitHub token (like research):
   'new-skill-name': GITHUB_TOKEN_PARAMS,

   // If it needs something unique:
   'new-skill-name': [
     {
       key: 'MY_KEY',
       description: 'What it is for',
       required: 'recommended',   // 'required' | 'recommended' | 'optional'
       link: 'https://where-to-get.it/',
       // group: 'my-group',  ← add only if AT LEAST ONE of multiple keys is enough
     },
   ],
   ```

**3. Rebuild and verify:**
   ```bash
   cd packages/octocode-skills
   node build.mjs
   node out/cli.js list
   node out/cli.js info <new-name>
   node out/cli.js list --json | python3 -c "import sys,json; s=json.load(sys.stdin)['skills']; [print(x['name'], x['env']['readiness']) for x in s]"
   ```

**4. Update README.md** — add a skill entry under `## Bundled skills` following the
   existing pattern: name, one-liner, when-to-use, env params table.

**No other files need to change.** `registry.ts` auto-discovers all SKILL.md folders;
`checker.ts` probes paths by name; `list/check/info/install` all call the shared modules.

---

## Task: Updating an existing skill

Changes to skill _content_ (SKILL.md, references, docs) in `../../skills/<name>/`:
- **No source change needed** — `build.mjs` syncs `skills/` at every build.
- Rebuild: `node build.mjs` — that's it.

Changes to skill _env requirements_:
- Edit `SKILL_ENV_PARAMS` in `src/env-params.ts`.
- Rebuild and run `node out/cli.js check <name>` to verify output.

Changes to skill _name or description_ (SKILL.md frontmatter):
- Edit frontmatter in `../../skills/<name>/SKILL.md`.
- `registry.ts:parseFrontmatter()` reads `name:` and `description:` — no code change.
- Rebuild and verify with `node out/cli.js list`.

---

## Task: Changing or adding a CLI command

### Adding a command

1. **Create** `src/commands/<command>.ts` exporting `run<Command>(opts): void`.
   - Accept a typed options object. Never read `process.argv` directly.
   - Always support `opts.json: boolean` — JSON goes to stdout, human to stdout, errors to stderr.
   - Exit code: `process.exitCode = 1` on failure (never `process.exit(1)` — it skips cleanup).

2. **Register** in `src/cli.ts`:
   - Add an `import` at the top.
   - Add a `case '<command>':` in the `switch` block.
   - Parse flags from `flags` / `positional` (already parsed by `parseArgs()`).
   - Add to the help text block in `printHelp()`.

3. **Export** from `src/index.ts` if it has programmatic use.

### Changing an existing command

- Each command is fully contained in its `src/commands/*.ts` file.
- `cli.ts` only parses args and dispatches — never put business logic there.
- Flag parsing is in `cli.ts`; flag _handling_ is in the command.
- To add a flag: add it to `cli.ts` dispatch `case`, pass it in the options object, handle it in the command, document it in `printHelp()`.

### Arg parser rules (`src/cli.ts:parseArgs`)

- `--key value` → `flags['key'] = 'value'` (next token not starting with `-`)
- `--key` alone → `flags['key'] = true`
- `-h` → `flags['help'] = true`
- Bare tokens → `positional[]`; first positional is command, rest go to the command

To read a string flag safely:
```ts
const raw = flags['my-flag'];
const val = typeof raw === 'string' ? raw : null;
```
To read a boolean flag:
```ts
const myFlag = Boolean(flags['my-flag']);
```

---

## Task: Changing output

### JSON output invariants

Every command's `--json` output must:
- Always have `"success": boolean` at the top level.
- Exit code 0 when `success: true`, non-zero when `success: false`.
- Never write anything other than a single JSON object to stdout.
- Use `process.exitCode = 1` (not `process.exit`) before returning.
- Errors: `{ "success": false, "error": "human message" }`.

**Stable JSON shapes per command:**

| Command | Top-level fields |
|---|---|
| `list --json` | `success`, `source`, `count`, `installedCount`, `skills[]` |
| `install --json` | `success`, `dryRun`, `override`, `skills[]`, `summary` |
| `check --json` | `success`, `skills[]`, `summary.install`, `summary.env` |
| `info --json` | `success`, `skill` (with `name`, `folder`, `description`, `dir`, `skillMd`, `env`) |

Each `skills[i]` in `list` carries: `name`, `folder`, `description`, `installed`, `linkedPlatforms`, `hasWorkspaceLink`, `hasBroken`, `env` (`readiness`, `params[]`, `hint`).

Each `skills[i]` in `check` carries: `name`, `installStatus`, `home`, `platforms[]`, `workspace`, `env`.

**Do not rename existing fields** — agents depend on them. Add new fields; never remove or rename.

### Human output rules

- All output goes to **stdout** (`console.log`). Only fatal parse errors go to stderr (`console.error`).
- Spinner goes to **stderr** so `--json` mode stays clean.
- Indentation: 2 spaces.
- Icons: `✓` green = good, `–` dim = absent/skipped, `⚠` yellow = warning, `✗` red = error, `→` green = symlink.
- Env icons: `✓` ready/ok, `⚠` partial, `✗` needs-config.
- Always end output with a blank line.
- Footer: actionable next-step commands, not just status.
- Color helpers are in `src/utils/colors.ts` — use only those, never ANSI escapes inline.

---

## Task: Changing env params

All env param knowledge lives exclusively in `src/env-params.ts`.

### Anatomy of an `EnvParam`

```ts
{
  key: 'TAVILY_API_KEY',          // exact env var name
  description: 'One line what it does',
  required: 'recommended',        // 'required' | 'recommended' | 'optional'
  group: 'web-search',            // optional — AT LEAST ONE in group satisfies the group
  link: 'https://...',            // where to get the key
}
```

### Group semantics

When multiple params share a `group`, the group is satisfied if **any one** of them is set.
This models "Tavily OR Serper OR Exa" — user only needs one of the three.

- `required` on each param in the group = the group itself is `required`
- `recommended` on each param = the group is `recommended`
- Do not mix levels inside a group.

### Adding a new shared param set

```ts
// in env-params.ts, before SKILL_ENV_PARAMS:
const MY_NEW_PARAMS: EnvParam[] = [
  { key: 'MY_KEY', description: '...', required: 'recommended', link: '...' },
];

// then in SKILL_ENV_PARAMS:
export const SKILL_ENV_PARAMS: Record<string, EnvParam[]> = {
  ...
  'my-skill': MY_NEW_PARAMS,
};
```

### `readiness` values and what triggers them

| Value | Meaning |
|---|---|
| `ok` | Skill has no env params at all |
| `ready` | All required groups satisfied, all standalone required params set |
| `partial` | Required ok, but ≥1 recommended param/group missing |
| `needs-config` | ≥1 required param or required group fully missing |

### What displays where

| readiness | `list` | `install` post-install | `check` | `info` |
|---|---|---|---|---|
| `ok` | nothing | nothing | "none needed" | "none needed" |
| `ready` | `✓ env ready` | nothing | `✓ env: ready` | "all set" |
| `partial` | `⚠ env partial · recommended: ...` | warning block | `⚠ env: ...` | yellow warning |
| `needs-config` | `✗ env missing · missing: ...` | warning block | `✗ env: ...` | red warning |

---

## Task: Changing the installation model

The install flow is entirely in `src/installer.ts:installSkill()`.

**Steps (in order):**
1. If `customPath` is set → install directly there (copy or symlink), skip home.
2. Otherwise → install to `getSkillsHome()/<name>/` (real copy via `copyDir`).
3. For each `platform` → symlink `getPlatformSkillsDir(platform)/<name>` → home.
4. If `workspace` → symlink `<cwd>/.agents/skills/<name>` → home.

**Key behaviors:**
- `force: true` (default) — removes existing target before writing.
- `force: false` (`--keep`) — skips if target exists; `homeStatus: 'skipped'`.
- `dryRun: true` — computes what would happen, returns the outcome object without touching disk.
- Windows symlinks use `junction` (directory junction, no admin required).

**To add a new installation target**, follow the `workspace` block pattern in `installSkill()`:
compute the dest path, call `createLink()`, push result into `links[]`.

**`checker.ts` is independent** — it only probes paths with `lstatSync`/`existsSync`. It is
never called by the installer; only by `list`, `check`, and (implicitly) post-install hints.

---

## Build & verify

```bash
cd packages/octocode-skills

# Full build (clean → sync skills → esbuild)
node build.mjs

# Typecheck only (no emit)
npx tsc --noEmit

# Smoke tests
node out/cli.js list
node out/cli.js list --json
node out/cli.js info octocode-research
node out/cli.js info octocode-research --json
node out/cli.js install octocode-research --dry-run
node out/cli.js install octocode-research --dry-run --json
node out/cli.js install octocode-research --platform pi --dry-run
node out/cli.js check
node out/cli.js check --json
node out/cli.js check octocode-research
node out/cli.js --help
```

**Acceptance criteria before concluding any change:**
1. `npx tsc --noEmit` → 0 errors
2. `node build.mjs` → `✓ Skills synced` + `✓ @octocodeai/skills built`
3. `node out/cli.js list --json` → parses cleanly, `success: true`
4. `node out/cli.js check --json` → parses cleanly
5. Human `list` and `check` outputs show expected badges and env hints

---

## Common pitfalls

| Pitfall | Cause | Fix |
|---|---|---|
| `SyntaxError: Invalid or unexpected token` on `node out/cli.js` | Double shebang: `src/cli.ts` has `#!/usr/bin/env node` AND `build.mjs` adds it via `banner` | Remove shebang from `src/cli.ts` — esbuild adds it |
| `TS2375 exactOptionalPropertyTypes` | Assigning `undefined` to an optional field | Use `...(x !== undefined ? { x } : {})` instead of `x: x` |
| New skill not appearing in `list` | `SKILL.md` missing or frontmatter malformed | Check `---` delimiters; `description:` must be quoted if it contains colons |
| Env param shows wrong readiness | Group logic wrong | Each group is satisfied by ANY set key; re-read `getSkillEnvStatus()` carefully |
| `out/cli.js` has stale code | Forgot to rebuild after source change | Always `node build.mjs` before testing |
| Spinner output in `--json` stdout | Spinner writing to stdout instead of stderr | Spinner must use `process.stderr.write()`; check `spinner.ts` |
| `process.exit(1)` kills cleanup | Hard exit skips `finally` blocks | Always use `process.exitCode = 1; return;` |
| Skills dir empty after build | `../../skills/` path not found | Run build from `packages/octocode-skills/`; check monorepo root has `skills/` |

---

## File ownership and what NOT to touch

| File | Rule |
|---|---|
| `out/` | Never edit — always generated |
| `skills/` | Never edit — always generated by `build.mjs` from `../../skills/` |
| `../../skills/` | Source of truth for skill content; edits there flow here at next build |
| `build.mjs` | Edit only to change what gets synced/excluded or add new esbuild entry points |
| `tsconfig.json` | Do not loosen `exactOptionalPropertyTypes` or `strict` — they catch real bugs |
| `package.json` `"files"` | Must include `"out/**"` and `"skills/**"` — remove either and publish breaks |

---

## Module responsibilities (one-line each)

| Module | Single responsibility |
|---|---|
| `registry.ts` | Read bundled `skills/` dir; parse SKILL.md frontmatter; return `SkillInfo[]` |
| `installer.ts` | Write files and symlinks; return `SkillInstallOutcome`; never read UI state |
| `checker.ts` | Read-only FS probe of install locations; return `SkillCheckResult`; never writes |
| `env-params.ts` | Static env param registry + runtime `process.env` probe; no FS I/O |
| `home.ts` | Resolve `OCTOCODE_HOME` / platform default; pure computation, no side effects |
| `platforms.ts` | Map platform name → directory path; parse comma-separated platform flag |
| `cli.ts` | Parse argv; dispatch to command; print help; no business logic |
| `commands/*.ts` | Orchestrate registry + checker + env-params + installer; format output |

Never put installer logic in commands. Never put output formatting in installer or checker.
Never import `commands/` from `registry.ts`, `installer.ts`, `checker.ts`, or `env-params.ts`.
Dependencies only flow inward: `commands → {registry, installer, checker, env-params, home, platforms}`.
