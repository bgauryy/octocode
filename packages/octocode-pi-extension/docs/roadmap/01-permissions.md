# 01 — Permission policy (allow / ask / deny)

**Status:** Proposed · **Priority:** P1 · **Owner area:** `src/permissions/` (new), `src/index.ts`, `src/subagents/`

## Problem and evidence

No tool call needs a decision today. Pi "does not ask for approval before every tool call" (Pi `docs/security.md:3`); its project trust "does not limit what tool calls can access or affect" (`docs/security.md:33`).

The `tool_call` pipeline has three gates (`src/index.ts:121-128`):

| Gate | Stops | Gap |
|---|---|---|
| `collab.reservationGate` (`src/team/tools.ts`) | Edits on paths another agent reserved | Coordination, not safety |
| `bashSafetyGate` (`src/files/bash-guard.ts:151`) | Wipe of `/` or home, disk format, power off, fork bomb | `git push --force`, `rm -rf src`, `curl … \| sh` pass |
| `hooks.preToolUse` (`src/hooks/register.ts:155`) | What a user hook refuses | Off by default, one process per call, only `deny` honored (`src/hooks/runner.ts:96`) |

File review (`src/files/review.ts`) has no rules. Read-only profiles are read-only by prompt only (`subagents/researcher.md:4`). Project trust (`src/shared/trust.ts:32`) decides which files load, not what a call may do. The user cannot say "never push", "ask before `npm publish`", or "this subagent may not run `git`".

## Competitor research

| Product | Decisions | Rule syntax | Precedence | Enforcement |
|---|---|---|---|---|
| Claude Code | allow / ask / deny; modes incl. `plan`, `bypassPermissions` | `Bash(npm run *)`, `Edit(/src/**/*.ts)`; paths `//abs`, `~/`, `/root`, `./cwd` | Deny from any scope wins; deny → ask → allow | Parsed command text; optional OS sandbox |
| Codex CLI | `approval_policy` × `sandbox_mode` | execpolicy `prefix_rule(…, decision=allow\|prompt\|forbidden)` | Strictest match wins | Seatbelt / Landlock; `writable_roots`; read-only `.git` |
| OpenCode | allow / ask / deny; `--auto` | `{"bash": {"git push *": "deny"}}`; `external_directory`, `doom_loop` | Last match wins; per-agent `permission:` frontmatter | Pattern only |

Sources:

- Claude Code <https://code.claude.com/docs/en/permissions>: "Rules are evaluated in order: deny, then ask, then allow"; "A rule must match each subcommand independently"; wrappers `timeout`, `nice`, `nohup` are stripped; "If a tool is denied at any level, no other level can allow it." Modes: <https://code.claude.com/docs/en/permission-modes>.
- Codex <https://github.com/openai/codex> at `822e58cc`: `codex-rs/protocol/src/config_types.rs:104`; `codex-rs/protocol/src/protocol.rs:1058-1077,1096-1100`; `codex-rs/sandboxing/src/seatbelt.rs:21-28`; `codex-rs/linux-sandbox/src/landlock.rs`; `codex-rs/execpolicy/README.md:5-8,95` ("strictest severity across all matches").
- OpenCode <https://opencode.ai/docs/permissions/>, <https://opencode.ai/docs/agents/>.

**Copy:** strictest wins; check every segment; strip a fixed wrapper list; per-profile rules; an outside-workspace class; protect policy files and `.git/hooks`.

**Avoid:** calling shell-text rules a sandbox (`sh -c`, `eval`, `/usr/bin/curl`, `xargs`, `find -exec`, `git -c` defeat them); `git *` (allows `git -c`); no word boundary (`ls*` matches `lsof`).

## Pi API constraints

| Need | Pi API | Note |
|---|---|---|
| Block | `pi.on('tool_call')` → `{ block, reason }` (`dist/core/extensions/types.d.ts:1046-1055`) | No `ask` result; the extension prompts. `terminate` stops the batch. |
| Ask | `ctx.ui.select(title, options, { signal, timeout })` (`types.d.ts:40-45,74-76`) | Only with `ctx.hasUI`. Precedent `src/files/review.ts:57-72`. |
| Nested calls | `ToolCallEvent.parentToolCallId` | Codemode calls also pass `tool_call`. |
| Settings | `pi.getSettings()` is read-only | Use Octocode files under `.octocode`. |
| Trust | `projectConfigFiles` (`src/shared/trust.ts:198`) | Add the project policy file. |

Pi has no engine to reuse (`examples/extensions/permission-gate.ts` is a three-regex confirm).

## Design

### Layers and schema

| Layer | Source | Loaded when |
|---|---|---|
| Built-in | `src/permissions/defaults.ts` | Always |
| Global | `<Octocode home>/permissions.json` | Always |
| Project | `<repo root>/.octocode/permissions.json` | Pi- and Octocode-trusted project; in the trust fingerprint |
| Profile | `permissions:` in subagent frontmatter | Child of that profile |
| Session | "Allow for this session" grants | In memory |
| Mode | `default` / `auto` / `plan` ([02](02-plan-mode.md)) | Command or `OCTOCODE_PERMISSIONS_MODE` |

```json
{
  "default": "allow",
  "rules": [
    { "tool": "bash", "command": "git push *", "action": "ask" },
    { "tool": "bash", "command": "git push --force*", "action": "deny", "reason": "Never force push" },
    { "tool": "bash", "command": "yarn test*", "action": "allow" },
    { "tool": "file", "path": "**/.env*", "action": "deny" },
    { "tool": "file", "path": "@outside", "action": "ask" },
    { "tool": "browser", "value": "evaluate", "action": "ask" },
    { "tool": "web", "value": "*.corp.example.com", "action": "deny" },
    { "tool": "mcp__github__*", "action": "ask" }
  ]
}
```

| Field | Meaning |
|---|---|
| `tool` | Required glob over tool names; `*` = all |
| `command` | Command pattern; `bash` only |
| `path` | Gitignore-style glob, `@outside`, or `@workspace` |
| `value` | Glob over the tool subject (below) |
| `action` | Required: `allow` \| `ask` \| `deny` |
| `reason` | Shown to the user; given to the model on deny |

A selector the tool lacks never matches; the loader warns.

Subjects (`src/permissions/subjects.ts`; reuse `mutatedPaths`, `src/team/routing.ts:136`):

| Tool | `path` | `value` |
|---|---|---|
| `file` | every `queries[].path` (strictest per batch) | `queries[].type` |
| `edit`, `write`, `read`, `grep`, `find`, `ls` | `path` (default cwd) | — |
| `mcp__octocode__local*`, `structureSearch`, `astSearch`, `astTopology`, `lspSearch` | every `queries[].path` / `workspaceRoot` | — |
| `browser` | `upload` paths | `action` |
| `web` | — | URL host, or `search` |
| `agent` | — | `profile` (or `general`) |
| `sendMessage` | — | `to` |
| `bash`, other MCP | — | — |

Paths resolve against the session cwd, then `realpath` where the file exists. Anchors copy Claude Code: `//abs/**`, `~/x/**`, `/x` (repo root), `x` or `./x` (cwd). `@workspace` is the repo root (or `OCTOCODE_TEAM_WORKSPACE`); `@outside` is everything else except the session scratch folder and `/tmp`.

### Built-in rules

| `file`/`edit`/`write` on | Action | Why |
|---|---|---|
| `~/.octocode/permissions.json`, `<repo>/.octocode/permissions.json`, `<repo>/.octocode/hooks.json`, `.pi/settings.json` | `ask` | The agent must not rewrite its policy |
| `**/.git/hooks/**`, `**/.git/config` | `ask` | Codex keeps them read-only |
| `@outside` | `ask` | OpenCode `external_directory` |

All else uses `default: allow`: existing users see no change.

### Bash command matching

Refactor the scanner in `src/files/bash-guard.ts` (`mask`, `words`, `substitutionEnd`, lines 28-125) into an exported `commandSegments(command): Segment[]`, shared by the catastrophic guard and the policy.

1. `mask` removes quotes and heredoc bodies and lifts `$(…)` and backticks into their own segments.
2. Split on `;`, `&&`, `||`, `|`, `|&`, `&`, newline, `(`, `)`.
3. Two forms per segment. **Strict** (allow) strips only `timeout <n>`, `time`, `nice`, `nohup`, `stdbuf …`, `command`, `builtin`, and `VAR=value` for `NODE_ENV`, `CI`, `FORCE_COLOR`, `LANG`, `LC_*`, `TZ`, `DEBUG`. **Loose** (deny/ask) also strips `sudo`, `doas`, `env` with options, every `VAR=value`, and flagless `xargs`.
4. A pattern is words with `*`. `git push *` matches `git push` and `git push origin main`; the space before a trailing `*` is a word boundary. Deny/ask compare the program basename (`/usr/bin/git push` matches); allow does not.
5. **Opaque segments** (`sh|bash|zsh|dash -c`, `eval`, `source`/`.`, `python|node|perl|ruby -c|-e`, `xargs` with flags, `find -exec|-delete`, `git -c`, unbalanced quote, dangling `&&`) never match allow. They take the default; with `default: allow` and any bash `ask` rule, they become `ask`.

Optional `"bashPaths": true` (off by default) applies path **deny** rules to path-like tokens (`cat .env`).

### Evaluation

```text
decide(call):
  if mode == plan: plan preset first (02); a plan deny returns at once
  matches  = rules from [built-in, global, project?, profile?, session]
             where tool glob and every given selector match
  # bash: deny/ask match any segment (loose); allow needs every segment (strict)
  decision = strictest(matches) or strictest(layer defaults)   # deny > ask > allow
  if decision == ask and (mode == auto or a session grant covers it): allow
  return decision, winning rule (layer, index, reason)
```

Precedence is by severity, not layer: project and profile `allow` rules never beat a `deny`/`ask`. Only a global rule with the same selector and `"override": true` widens a built-in `ask`. Project files and profiles cannot use `override` or `auto`.

Pipeline order (`src/index.ts:121`): `[collab.reservationGate, bashSafetyGate, permissionGate, hooks.preToolUse]`; hooks run only for allowed calls. In M4 a hook's `permissionDecision: "ask"` uses the ask flow.

### Ask UX

`ctx.ui.select` with the call's `signal`; the sanitized body names tool, subject, rule and layer:

| Option | Effect |
|---|---|
| Allow once | Run this call |
| Allow for this session | Grant: bash → first two words of the segment + ` *` (shown); paths → exact path; others → tool + value |
| Always allow | Append the grant to the global file (atomic, `src/shared/atomic.ts`); never to the project file |
| Deny | Block: `Denied by the user.` |
| Deny and say why | `ctx.ui.input` text goes into the reason |

Dismiss or abort = deny. A multi-path `file` batch asks once.

### Commands and env

| Command / env | Effect |
|---|---|
| `/octocode permissions` | Rules by layer, mode, grants, loaded files and errors |
| `/octocode permissions mode default\|auto\|plan` | Set mode; footer `perm: auto` (warning) / `plan` |
| `/octocode permissions check <tool> <subject>` | Dry run: decision and winning rule |
| `… forget` / `… reload` | Drop grants / reload files |
| `OCTOCODE_PERMISSIONS_MODE` | Start mode |
| `OCTOCODE_PERMISSIONS=0` | Gate off (debug; footer warning) |
| `OCTOCODE_PERMISSIONS_POLICY` | Policy JSON for a child (set by the parent) |
| `OCTOCODE_PERMISSIONS_RELAY_SECONDS` | Relay wait, default 120 |

### Subagents

A subagent runs headless (`--mode json`, `src/subagents/process.ts:79-93`).

1. **Transfer.** The parent puts its effective policy (built-in, global, project, mode; no session grants) plus the profile's `permissions` into `OCTOCODE_PERMISSIONS_POLICY`. The child reads only this value, so a worktree child gets the same rules.
2. **Profiles tighten only.** Profile rules join the strictest-wins merge. Pi's `parseFrontmatter` uses the full `yaml` parser (Pi `dist/utils/frontmatter.js`, `parse(yamlString)`); nested block and flow maps under `permissions:` parse to objects (verified). `parseProfile` (`src/subagents/profiles.ts:50`) validates it.

   ```markdown
   ---
   name: researcher
   excludeTools: file,browser
   permissions:
     rules:
       - { tool: bash, command: "git push *", action: deny }
       - { tool: bash, command: "rm *", action: ask }
   ---
   ```

3. **Child `ask`.** Until M4: deny with `Needs the user's approval: <rule>. Ask your parent with sendMessage, or report it as a blocker.` M4 relay: if the parent has a UI, the child sends a `permission-request` team message and waits up to `OCTOCODE_PERMISSIONS_RELAY_SECONDS`. The parent shows the dialog with the child's id and profile; a session grant covers only that child. Timeout, no UI, or parent gone → deny.
4. **Message `kind`.** One agent-DB migration v1→v2 (`src/agentdb/schema.ts`) adds a team message `kind` column: `message | interrupt | permission-request`; old rows read as `message`. It ships with whichever lands first, [04](04-agents-view.md) interrupt or this relay; the other reuses it.
5. **Headless main session** (`pi -p`, RPC without UI): `ask` → deny. Override: `OCTOCODE_PERMISSIONS_MODE=auto`.
6. **Fan-out** ([05](05-fan-out-batches.md)): items are normal children with the merged policy; use a profile, not a batch-level policy.

### Prompt

The `octocode` section does not change. Block reasons carry the rule and "do not retry; choose another approach or ask". When any `ask`/`deny` rule exists, bash and file `promptGuidelines` get: "A refusal names the user's permission rule; do not work around it." Guidelines change only at session start, so the cache holds.

### Failure modes

| Case | Behavior |
|---|---|
| Invalid file | Skip it, keep other layers, warn once (like `src/hooks/register.ts:129`) |
| Dialog dismissed or aborted | Deny |
| Gate throws | Block `Permission check failed: <error>` (fail closed) |
| File edited mid-session | Reload on `session_start` or `reload`; fingerprint catches project edits |
| Loop | 3 identical denials in one turn → block with `terminate: true` (OpenCode `doom_loop`) |

### Security notes

- A **policy on what the model writes**, not a sandbox: it cannot see inside scripts or `npm test`. Say so in docs and dialog; recommend an OS sandbox (Pi `docs/containerization.md`) for untrusted repositories.
- Dialog text passes `sanitizeTerminalText`.
- The project file loads only after trust; its allows cannot beat deny/ask; it cannot set `auto` or `override`. Built-in `ask` rules protect the policy files.

## Files to change

| Path | Change |
|---|---|
| `src/permissions/schema.ts` (new) | Types, validation |
| `src/permissions/load.ts` (new) | Global, trusted project, child env policy |
| `src/permissions/match.ts` (new) | Globs, anchors, command patterns, strictest-wins |
| `src/permissions/subjects.ts` (new) | Subject extraction |
| `src/permissions/gate.ts` (new) | Gate, dialog, grants, auto, loop guard |
| `src/permissions/command.ts` (new) | `/octocode permissions …` |
| `src/files/bash-guard.ts` | Export `commandSegments` |
| `src/index.ts` | `permissionGate` at line 121; command |
| `src/shared/home.ts` | `HOME_NAMES.permissions = 'permissions.json'` |
| `src/shared/trust.ts` | Policy file in `projectConfigFiles` |
| `src/shared/env.ts` | New env names |
| `src/subagents/profiles.ts` | Validate `permissions` frontmatter |
| `src/subagents/process.ts` | Pass `OCTOCODE_PERMISSIONS_POLICY` |
| `src/agentdb/schema.ts` | Migration v1→v2: message `kind` (shared with 04) |
| `src/team/model.ts`, `store.ts`, `session.ts` | M4: `permission-request` kind and answer delivery |
| `tests/architecture.test.ts` | `permissions/` above `shared/`, `files/`, below `index.ts` |
| `docs/CONFIGURATION.md`, `docs/FEATURES.md`, `README.md` | Permissions section, env, commands |

## Phased plan

| Milestone | Scope | Ships |
|---|---|---|
| M1 Engine + deny | Schema, loader, matcher, `commandSegments`, deny-only gate, `/octocode permissions`, `check` | Users can forbid calls |
| M2 Ask | Dialog, grants, `auto`, built-in protected paths, headless ask → deny, loop guard | Interactive approval |
| M3 Subagents | Profile `permissions`, env transfer, bundled profile rules (researcher/reviewer deny `git commit/push`, `rm`; plan preset in 02) | Enforced read-only profiles |
| M4 Relay + hooks | Migration v1→v2 unless 04 shipped it; child → parent relay; hook `ask`/`allow`; optional `bashPaths` | Child approvals |

## Test plan

**Unit (`tests/permissions/*.test.ts`):**

- Matcher: `git push *` vs `git push`, `git pushx`, `cd x && git push`, `echo "$(git push)"`, `FOO=1 git push`, `sudo git push`, `/usr/bin/git push`, `timeout 5 git push`; `ls *` vs `lsof`.
- `yarn test && curl x | sh` with only `yarn test*` allowed → default/ask.
- Opaque: `bash -c 'git push'`, `eval`, `python -c`, `find -exec`, `git -c core.fsmonitor=x status`.
- Layers: project allow loses to global ask; profile cannot widen; `override` and `auto` ignored outside global.
- Anchors `//`, `~/`, `/`, `./`, `@outside`; symlink escape.
- Subjects per tool row.
- Profile `permissions:` block and flow maps validate; bad shape rejected.
- Loader: invalid file skipped; untrusted project skipped; fingerprint changes on edit.
- Gate: headless ask → deny; aborted dialog → deny; throw → block.
- Agent DB: v4 → v5; old rows read as `message`; `kind` round-trips.

**End-to-end (scripted model, real Pi session):** deny on `git push` returns the reason, marker file absent; via RPC extension UI (Pi `docs/rpc-extension-ui.md`), allow once asks again, allow for session does not; a profile rule blocks a child's `git commit`.

**Real Pi flow:** `yarn build`; `pi --no-extensions -e packages/octocode-pi-extension/dist/index.js -e builtin:mcp -e builtin:tool-search` with a global `ask` on `git push *`. Ask the model to push; check the dialog, footer, `check`, and a `researcher` child trying `git commit`.

## Open questions

1. "Always allow" into the project file for trusted projects? Proposed: no; a shared file needs a reviewed change.
2. `read` on `**/.env*` (OpenCode denies)? Proposed: `allow` in M1–M2; built-in deny in M3 with a release note.
3. `bash` `background: true` as a selector? Proposed: M4, on request.

## Out of scope

OS sandbox for bash (tracked separately); bash network policy; managed or remote policy; `user_bash` (`!cmd`, catastrophic guard and hooks only); plan mode details ([02](02-plan-mode.md)).
