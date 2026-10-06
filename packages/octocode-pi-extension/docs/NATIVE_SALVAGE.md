# Salvaging the native packages into the Pi extension

> **Historical record.** The native packages (`packages/octocode-agent`,
> `octocode-agent-core`, `octocode-agent-core-rust`, `octocode-agent-contracts`,
> `octocode-agent-testing`) have been removed from this repository. This document is
> kept as a record of what was ported into the extension; paths and commands below that
> name those packages no longer exist.

Implementation spec for moving the useful parts of
`packages/octocode-agent` (native host) and
`packages/octocode-agent-core-rust` (Rust services) into this extension, so the
native packages can later be removed.

> **Superseded since:** Octocode's own MCP client (hub, `mcp` tool, pagination, redirects, OAuth and
> `pi-mcp-auth/`, resources and prompts) was removed; the extension now registers only the built-in
> `octocode` server with Pi's built-in MCP (`src/mcp/octocode.ts`), which owns every other server.
> `OCTOCODE_TEAM_DB` is gone (the team tables live in the agent database); the custom footer
> and banner were removed and later restored in `src/ui/`. Sections 1.4, 1.6, 1.7, 2.1 and 2.2 below describe removed code.

> **Status: implemented** (all phases). Deviations from the text below:
> - MCP pagination comes from the SDK (`listMaxPages: 50` in `mcp/host.ts`), not a hand loop.
> - OAuth never opens a browser at startup; a 401 without a refresh token asks the user to run `/mcp login <server>`.
>   Config is `"oauth": true | false | { clientId, scopes }`; `/mcp logout` deletes local credentials only.
>   Credential writes are serialized with a cross-process lockfile; a losing refresh never deletes a winner's tokens.
> - Web SSRF guard checks every resolved address and every redirect hop, but does not pin DNS (that needs `undici`);
>   DNS rebinding between check and connect remains a documented residual risk.
> - Background subagents are cancelled with `/agents kill <id>`. Isolated refs never overwrite an
>   unmerged ref: a reused id saves to `<id>-2`, `<id>-3`, …
> - Isolated subagents join their parent's team via `OCTOCODE_TEAM_WORKSPACE`.
> - Hooks are opt-in with `OCTOCODE_HOOKS=1`; timeouts kill the hook's whole process group.
> - Team reservations fail closed only for sessions that joined the team or hold leases.
> - Files over 4 MiB use size + mtime for the stale-read check; smaller files use sha256.
> - Checkpoint turns are user prompts; checkpoints live in `<Octocode home>/agent/pi/sessions/<session>/checkpoints/`
>   and are removed with their session's folder by the sessions sweep (see the README's Retention table).

## Ground rules

- **Adapt, never import.** The extension must not depend on `octocode-agent`,
  `agent-core`, `agent-contracts`, `@octocodeai/config` or a Rust binary.
  Every item below is re-implemented in TypeScript inside `src/`.
- **No Rust binary.** Shipping one means about 6 platform builds, macOS signing
  and a supervised child process on every install. The only thing Rust adds
  beyond TypeScript is descriptor-relative (symlink-proof) writes. That is not
  a security boundary in an extension that also exposes `bash`.
- **Zero or near-zero prompt cost.** Fixes add no tools and no schema text.
  New capabilities extend existing tools (`mcp`, `agent`) instead of adding
  tools.
- **Build on Pi.** Skip anything Pi already owns: models, providers,
  sessions, `/fork`, compaction, settings, commands, editor, skills, RPC.
- **Out of scope (decided):** no plan tool, todo tool or task tracking, no DAG
  or task scheduling, no automations or scheduled runs, and no MCP `tasks`
  support.
- Keep the folder layering enforced by `tests/architecture.test.ts`. Each new
  module lives in the folder that owns the domain.

## Phase 1: correctness and security fixes

These are small, independent fixes with no new tools. Each can ship as its own
change.

### 1.1 The team reservation check fails closed

- **Problem:** `src/team/session.ts:164-171` `blockedBy()` returns `undefined`
  on any database error (`SQLITE_BUSY`, closed DB). The `file` tool then edits
  a path another agent holds.
- **Borrow:** the fail-closed rule from `core-rust/ARCHITECTURE.md`.
- **Change:**
  - Keep the "no team DB file yet → not blocked" fast path.
  - On any error from `conflict()`, return a refusal instead: `Team database
    unavailable (<code>); retry the change.`
  - `reservation()` must pass that text through.
  - Do not fail closed when this process has never joined and no DB exists.
- **Tests (`tests/team.test.ts`):** a store whose `conflict` throws makes
  `reservation()` return a refusal, and the `file` tool reports it.

### 1.2 File leases detect loss (lease fencing)

- **Problem:** `src/team/store.ts:378-380` `renew()` silently skips leases
  that already expired. After a heartbeat stall of more than 90 s a peer can
  lock the path while this agent keeps editing it.
- **Borrow:** the generation or fencing idea from `core-rust/src/lib.rs`
  (`communication_claim` and `work_heartbeat`, around lines 3034-3135 and
  4050), reduced to one process per agent.
- **Change:**
  - `renew(owner, now)` returns the paths of the owner's leases that expired
    before renewal. Select first, then update, inside `transaction()`.
  - `TeamSession.flush()` reports lost paths once, through `pi.sendMessage` (a
    custom message: "Your reservation on X lapsed; lock it again before
    editing"), and drops them from its local view.
  - Before each `file` mutation, the check also confirms that a live lease
    this agent believes it holds still exists. If it lapsed, refuse and ask
    for a re-lock.
- **Not needed:** a generation column. There is one writer per owner id, so
  "lease row still live" is the token.
- **Tests:** an expired own lease is reported by `renew`. After a takeover by
  another owner, the `file` tool refuses.

### 1.3 Two-phase message delivery and dead letters

- **Problem:**
  - `src/team/store.ts:291-306` `claim()` marks rows delivered before
    `pi.sendMessage` runs (`src/team/session.ts:226, 241-249`), and that
    error is swallowed. The message is lost, but `recent()` says "delivered".
  - `remove()` (`store.ts:244-249`) deletes unread deliveries, so the sender
    sees "dropped" or nothing.
- **Borrow:** the delivery states from `core-rust/src/lib.rs:247` and
  `recover_sealed` (`lib.rs:2272-2339`), simplified.
- **Change:**
  - Split delivery into `pending(id)` (read undelivered, no write) and
    `ack(id, messageIds, now)` (set `delivered_at`, and `completed_at` for
    FYI messages).
  - `tick()` delivers each message, then acks only the ones whose
    `sendMessage` did not throw. A message that failed stays pending for the
    next tick.
  - There is one reader per recipient id, so no lease is needed.
  - Add a `dead_at INTEGER` column to `deliveries` (see 1.4 for the
    migration). `remove()` sets `dead_at` on undelivered rows instead of
    deleting them.
  - `recent()` reports `dead-lettered`, meaning the recipient left before
    reading.
  - `send()` refuses recipients that are no longer live and returns their
    ids, so `sendMessage` can say so right away.
- **Tests:** a throwing `sendMessage` leaves the row pending and it is
  delivered on retry. Leaving marks rows dead and `recent()` shows
  `dead-lettered`. Sending to a departed id is refused.

### 1.4 Team DB identity and schema version

> Superseded: the team tables moved into the agent database (`OCTOCODE_AGENT_DB`, schema v3 with migrations).

- **Problem:** `src/team/store.ts:198-206` runs `CREATE TABLE IF NOT EXISTS`
  on any file (`OCTOCODE_TEAM_DB`). An old schema half-works because `tick()`
  swallows the errors, and a foreign database gets our tables added to it.
- **Borrow:** `lib.rs:762-792` (application id) and `lib.rs:944-956`
  (schema shape check).
- **Change:**
  - In `open()`, read `PRAGMA application_id` and `user_version`.
  - A new file gets a fixed `application_id` and `user_version = N`.
  - Numbered migrations in one transaction, as plain `ALTER TABLE` steps (the
    first one adds `deliveries.dead_at`).
  - A foreign `application_id`, or a `user_version` newer than the code
    knows, throws a clear error that the team layer surfaces once in `/agents`
    and the footer.
- **Tests:** a fresh DB gets the id and version, a v1 DB migrates, and a
  foreign or newer DB is refused.

### 1.5 Atomic writes with a content-digest precondition

- **Problem:**
  - `src/files/tool.ts:23-58` detects staleness by `mtimeMs` plus `size`, so
    a same-size rewrite within the mtime granularity passes (formatters,
    `git checkout`).
  - Pi's edit and write tools write in place, so a crash or a concurrent
    reader can see a truncated file.
  - There is a time-of-check to time-of-use gap between `guard.check` and the
    write.
- **Borrow:** `core-rust/src/fs_service.rs:649-712` `replace` and
  `:1560-1589` sha256 preconditions.
- **Change:**
  - New `src/files/atomic.ts` exports
    `atomicWriteFile(file, data, expectedSha?)`: write a temp file in the same
    directory (`.<name>.<pid>.<rand>.tmp`), `FileHandle.sync()`, re-hash the
    target and compare with `expectedSha`, keep the existing file mode,
    `rename`, sync the parent directory (skip on Windows), and remove the temp
    file on failure.
  - `FileGuard` stores `{ mtimeMs, size, sha256 }`. Keep the mtime and size
    check as the fast path and hash only when they match (the cheap reject
    stays cheap).
  - Pass the atomic writer to Pi through `options.operations` on
    `createEditToolDefinition` / `createWriteToolDefinition` (Pi
    `dist/core/tools/edit.js:81` and `write.js:21`).
  - The operations `readFile` records the digest, and `writeFile` enforces it.
- **Tests (`tests/files.test.ts`):** a same-size, same-mtime rewrite is
  refused. No partial file appears when the write throws midway. The mode is
  kept.

### 1.6 Paginate MCP tool lists

> Superseded by Pi's built-in MCP; this code was removed.

- **Problem:** `src/mcp/hub.ts:236` reads only the first `listTools()` page.
- **Borrow:** the pagination loop in native `src/mcp/mcp.ts`.
- **Change:** loop on `nextCursor`, capped at 50 pages, and treat a repeated
  cursor as the end.
- **Tests (`tests/mcp.test.ts`):** a stub server with 2 pages registers every
  tool.

### 1.7 Same-origin redirects for the MCP HTTP transport

> Superseded by Pi's built-in MCP; this code was removed.

- **Problem:** the HTTP transport uses the default `fetch`, so configured
  `headers` (bearer tokens) follow cross-origin redirects.
- **Borrow:** `octocode-agent/src/mcp/mcp.ts:969-986`
  (`sameOriginRedirectFetch`, 18 lines).
- **Change:** new `src/mcp/fetch.ts`, passed as `fetch` to
  `StreamableHTTPClientTransport` in `src/mcp/host.ts`. It uses
  `redirect: 'manual'`, follows at most 3 redirects, keeps the method, and
  refuses a redirect to another origin.
- **Tests:** a cross-origin 302 is refused and a same-origin one is followed.

### 1.8 Block private addresses in `web`

- **Problem:** `src/web/web.ts` fetches any URL: localhost, LAN addresses,
  `169.254.169.254` (cloud metadata). A prompt-injected page can steer the
  model there.
- **Borrow:** `octocode-agent/src/tools/web-tool.ts:79-178, 246-258`
  (`isBlockedWebIp`, `assertPublicUrl`, redirect checks for each hop).
- **Change:**
  - New `src/web/guard.ts` blocks loopback, private, link-local, CGNAT,
    multicast and reserved IPv4 addresses. It blocks IPv6 loopback, ULA and
    link-local addresses, and IPv4 embedded in IPv6.
  - Resolve with `dns.lookup(host, { all: true })` and refuse if any answer
    is blocked.
  - Follow redirects manually (at most 5) and check every hop.
  - The opt-out `OCTOCODE_WEB_ALLOW_PRIVATE=1` covers local development.
  - The `browser` tool is **not** affected: it drives the user's Chrome on
    purpose. (Later changed: `browser` now asks before opening private hosts.)
  - DNS pinning with an `undici` dispatcher, as the native host does, stops
    DNS rebinding. Add it only if it needs no new dependency. Otherwise note
    the remaining rebinding risk in the README.
- **Tests (`tests/web.test.ts`):** a table test over address ranges, and a
  redirect to a private address is refused.

## Phase 2: capabilities

### 2.1 MCP OAuth for remote servers

> Superseded: Pi's built-in MCP handles OAuth (`/mcp login`); Octocode's OAuth code was removed.

- **Gap:** `src/mcp/host.ts` passes no `authProvider`, so servers that only
  offer OAuth cannot connect.
- **Borrow:** `octocode-agent/src/mcp/oauth.ts:212-372`
  (`createNativeMcpOAuthFlow`): an `OAuthClientProvider` implementation,
  PKCE, and a 127.0.0.1 callback server that checks state, `Host` and path
  and sends no-store and CSP headers. Also its revoke and invalidate logic.
- **Change:**
  - New `src/mcp/oauth.ts`.
  - Store tokens, client info and the verifier in
    `<Octocode home>/pi-mcp-auth/<server-hash>.json` with mode `0600`.
  - Drop the native keychain code (`oauth.ts:51-110`) unless asked for.
  - Before opening the browser, ask for approval with `ctx.ui.confirm`. In
    print or subagent mode, fail with "run `/mcp login <server>`
    interactively".
  - Commands: `/mcp login <server>` and `/mcp logout <server>`.
  - Config: `"oauth": true` or automatic on a 401 challenge, plus optional
    `clientId` and `scopes`.
  - The callback server listens only while a login is pending and times out
    after 5 minutes.
- **Tests:**
  - The stub HTTP MCP server issues a 401, then the auth code flow runs
    against a local fake authorization server. The token is persisted and
    reused on reconnect.
  - `logout` deletes the token file.
  - A state mismatch is refused.

### 2.2 MCP resources and prompts (on the existing `mcp` tool)

> Superseded: the `mcp` tool was removed; Pi's built-in MCP exposes resources.

- **Gap:** the hub only lists and calls tools.
- **Borrow:** the action set in `octocode-agent/src/mcp/mcp.ts:1070-1130`,
  without `task-*`.
- **Change:**
  - Add these actions to the `mcp` tool:
    - `resources`: list, paginated.
    - `read-resource`: text is capped like tool output, at most 4 images.
    - `prompts`: list.
    - `get-prompt`: returns the messages as text.
  - The schema gains one enum extension and two optional fields (`uri`,
    `prompt` with `args`). Keep the descriptions to one line.
  - Only list servers whose negotiated capabilities include resources or
    prompts.
- **Tests:** the stub server exposes one resource and one prompt, and both
  actions return them.

### 2.3 Edit checkpoints and rewind

- **Gap:** there is no undo for agent edits.
- **Borrow:**
  - The idea of the checkpoint journal in `core-rust/src/fs_service.rs:738-935`
    (content-addressed before and after images, rewind verifies the
    after-image, hard caps).
  - The flow of Pi's `examples/extensions/git-checkpoint.ts`.
- **Change:**
  - New `src/files/checkpoint.ts`.
  - Before each successful `file` mutation, store the pre-image once per
    turn and path. Store its sha256 blob under
    `<Octocode home>/agent/pi/sessions/<session>/checkpoints/blobs/` and append
    `{ turn, path, beforeSha | absent, afterSha | absent, mode }` to a JSONL
    journal.
  - Caps: 128 MiB and 256 files per session, dropping the oldest turns first
    (same limits as the Rust journal). Skip files over 8 MiB with a note.
  - `/rewind [turns]` restores in reverse order. It first checks that each
    file's current digest equals the recorded `afterSha`. A diverged file is
    skipped and listed, never overwritten.
  - Hook Pi's `session_before_fork` (as `git-checkpoint.ts` does) to offer
    restoring files to the fork point.
  - This covers untracked and non-git files, which a `git stash` approach
    misses.
  - Writes made through `bash` are not captured. Say so in the README.
- **Tests:** edit, write-new and delete are each rewound correctly. A
  diverged file is skipped. Pruning to the caps works.

### 2.4 Git worktree isolation for implementer subagents

- **Gap:** parallel implementer subagents share one working tree and rely on
  cooperative leases.
- **Borrow:** `octocode-agent/src/work/worker-worktrees.ts:155-243` (create,
  commit, private ref), plus a reduced form of `worker-integration.ts`
  (merge with hooks disabled, conflict list).
- **Change:**
  - New `src/subagents/worktree.ts` and an opt-in `isolate: true` field on
    `agent`. The field description is one line and the default is off.
  - **Create:** `git rev-parse HEAD`, then
    `git worktree add --detach <octocode-home>/pi-worktrees/<repo-hash>/<agent-id> <oid>`.
    Pass that path as the `cwd` to `runSubagent` (`src/subagents/process.ts:87`
    already takes one).
  - **On finish:** if the tree changed, `git add -A`, commit, and store the
    result under `refs/octocode/pi/<agent-id>`. The report ends with the ref,
    `git diff --stat` and a `git merge` hint.
  - **Merge:** keep it manual by default. `/agents merge <agent-id>` runs
    `git -c core.hooksPath=/dev/null merge --no-ff <ref>` in the main tree.
    On conflict it aborts and lists the conflicting paths.
  - **Cleanup:** `git worktree remove --force`. At startup, prune worktrees
    whose owning agent is gone.
  - Refuse `isolate` outside a git repository, and refuse it when the main
    tree has uncommitted changes that touch the task's paths. The simpler
    rule, "warn when the tree is dirty", is acceptable.
  - Document that ignored files (`node_modules`, builds) are absent in the
    worktree.
- **Tests (e2e with a temp repo):** an isolated subagent's change lands on
  the ref and not on the main tree. The worktree is removed, and a conflict
  is reported.

## Phase 3: small hardening items

| Item | Borrow | Change |
|---|---|---|
| Bash safety hook | `octocode-agent/src/tools/bash-tool.ts:7-71` | A Pi `tool_call` guard beside `src/files/read-guard.ts` refuses catastrophic commands (`rm -rf /` or `~`, `mkfs`, `dd of=/dev/…`, fork bombs, shutdown). Do not strip environment variables, because that breaks `gh` and `npm` auth. |
| Abort a background subagent | `work/worker-tool.ts:42-52` | Add an `abort` action or a `/agents kill <id>` command that sends SIGTERM to the child, reports it as cancelled and frees the slot. |
| Input caps | `work/worker-tool.ts:33-40` | Cap the `agent.task` and `sendMessage.message` lengths in the schema (`maxLength`). |
| Terminal text sanitising | `interactive/composer.ts:37-62` | Strip OSC and CSI escape codes, C0 controls and bidi overrides from subagent and MCP text before `src/shared/render.ts` draws it. |
| Claude/Codex hooks (optional) | `extensions/hook-dispatcher.ts:15-72`, `extensions/adapters.ts:89-125` | Map `hooks.json` `PreToolUse`, `PostToolUse`, `SessionStart` and `PreCompact` command hooks onto Pi events, with a 16 KB context cap. Project hooks load only in trusted projects. |

## Explicitly not ported

| Source | Reason |
|---|---|
| `work/plan.ts`, `plan-context.ts`, `worker-dag-scheduler.ts`, `automation-scheduler.ts`, MCP `task-*` | Out of scope by decision (no plan, task or schedule features). |
| `providers/*`, `context/prompt.ts`, `context/token-meter.ts`, `sessions/*`, `settings/*`, `interactive/*` (except the sanitiser regexes) | Pi owns models, context files, sessions, `/fork`, settings, commands and the editor. |
| `transports/acp.ts`, `transports/transports.ts`, `api/*` | Pi has RPC mode, and `pi-acp` already adapts ACP. |
| `tools/file-tool.ts`, `tools/ffmpeg-tool.ts`, `tools/artifact-*` | These depend on the Rust filesystem service or the native TUI. `bash` covers FFmpeg. |
| `tools/registry.ts`, `extensions/skills.ts`, `customization*.ts`, `pi-session-import.ts` | The extension's `mcp/facade.ts` and `skills.ts`, and Pi's skill loader, already cover these. |
| `terminal/opentui/*` | A different renderer. At most, add capacity counts (active/max) to `src/team/panel.ts`. |
| Rust effect ledger, work graph, automations, worktree and handoff tables, session paging, JSONL actor protocol, `openat` containment, Windows handle resolver | These are native-host durability machinery with no consumer in Pi. |

## Delivery order and budget

| Step | Items | New code, approx. | New prompt text |
|---|---|---|---|
| 1 | 1.1, 1.6, 1.7, 1.8 | 200 lines | none |
| 2 | 1.2, 1.3, 1.4 | 150 lines | none |
| 3 | 1.5 | 120 lines | none |
| 4 | 2.1 | 300 lines | none (commands only) |
| 5 | 2.3 | 200 lines | none (command only) |
| 6 | 2.2 | 100 lines | a few enum values and 2 fields on `mcp` |
| 7 | 2.4 | 200 lines | 1 optional field on `agent` |
| 8 | Phase 3 | 150 lines | about 1 line |

Each step must pass:

1. `yarn workspace @octocodeai/pi-extension lint`
2. `yarn workspace @octocodeai/pi-extension test` (unit and e2e)
3. `yarn workspace @octocodeai/pi-extension build`
4. A real run: `pi --no-extensions -e packages/octocode-pi-extension/dist/index.js`
   exercising the changed flow.

Update `README.md` (the feature table and environment variables) in the same
change.

## Removing the native packages afterwards

Only `octocode-agent` consumes `agent-core`, `agent-contracts` and
`agent-testing`. Once this spec has landed, removing
`octocode-agent` and `octocode-agent-core-rust` also removes those three. The
root `package.json` scripts would change too: `test:rust-integration`,
`test:ffi`, `test:pack`, `test:production-conformance`, `test:pty*`,
`test:performance*` and `verify`. So would the root `AGENTS.md` ownership
table. Those changes are a separate decision and are not part of this spec.
