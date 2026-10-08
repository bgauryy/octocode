# Host hook event contracts

Nothing changes user-level host settings. Native shell hooks live in host configuration; OPERATING.md frontmatter installs no hooks.
Installation, merge locations, trust, and a delivery check: [host setup](HOST_SETUP.md#install-messaging-hooks). Use `host-config --vendor claude|codex|grok|cursor` to preview settings; native delivery and hook delivery are alternatives for one identity.

## Event contracts

| Host | Identity/presence events | Inbox context | Not for delivery |
| --- | --- | --- | --- |
| Claude Code | `SessionStart`, `UserPromptSubmit`, tool events; `SessionEnd` leaves | `PostToolUse`/`PostToolUseFailure` `hookSpecificOutput.additionalContext` | Prompt events maintain presence; session-start context carries binding only |
| Codex | `SessionStart`, `UserPromptSubmit`, `PostToolUse`; `SessionEnd` leaves | `PostToolUse` `hookSpecificOutput.additionalContext` | No `PostToolUseFailure` event configured; session-start context carries binding only |
| Cursor | `sessionStart`, `beforeSubmitPrompt`, tool events; `sessionEnd` leaves | `postToolUse`/`postToolUseFailure` `additional_context` | `beforeSubmitPrompt` only validates |
| Grok Build | `SessionStart`, `UserPromptSubmit`, tool events; `SessionEnd` leaves | `PostToolUse`/`PostToolUseFailure` `hookSpecificOutput.additionalContext` | Session-start and prompt-hook output is discarded |
| OpenCode | Existing session ID via server/SDK | `POST /session/:id/message` with `noReply:true` | No shell hook needed when the server is reachable |

**Cursor** ([hooks reference](https://cursor.com/docs/hooks)): `sessionStart` is asynchronous and injects only initial identity context, never peer messages. Skill-hook frontmatter is not assumed to run, and executable names do not identify a vendor (Grok also installs `agent`). Contract-fixture-tested, not live-model verified.

**Grok** ([hook contract](https://github.com/xai-org/grok-build/blob/f0e3be1100ef5252488e3be8bb0e91cf68d8c305/crates/codegen/xai-grok-pager/docs/user-guide/10-hooks.md)) uses camelCase payload keys, PascalCase compatibility events and project hook locations.
- Grok project hooks require a trusted Git project root. Creating `.grok/hooks/` alone does not establish one. Inspect the installed host before enabling them. Check `projectRoot`, `projectTrusted` and `hooks` first.
- The adapter inlines Grok context up to 8,000 UTF-8 bytes and bounds output at 9,000 bytes. Larger messages retain the full record envelope with `data.bodyOmitted:true` and executable `data.next` fetch instructions. It defers excess rows to a later event without trimming bodies or acknowledging them.
- Delivery is post-tool: `PreToolUse` context drops when a tool is denied.

**Claude/Codex** use snake-case `session_id`, `cwd`, and `hook_event_name`; parent hooks ignore `agent_id` child events. Each participating child needs a separate binding. Codex previews and emitted context share a 5,000 limit; the adapter bounds UTF-8 bytes conservatively and sends full-body DB references when needed. Batches that still exceed the limit defer remaining items to a later event. Recover bodies by returned references. See [Claude hooks](https://code.claude.com/docs/en/hooks) and [Codex hooks](https://learn.chatgpt.com/docs/hooks).

## OpenCode: direct API before hooks

The [server API](https://opencode.ai/docs/server/) accepts `noReply`; in [the inspected source](https://github.com/anomalyco/opencode/blob/16c56fe5ecc3305028d1f0a9cff5806e51c9d480/packages/opencode/src/session/prompt.ts) `createUserMessage` runs before the `noReply` return, so the message is stored without a model turn (passive delivery, like Codex injection). Keep DB stage/confirm/complete around it, never send peer text as a `system` override, and reply through the CLI/DB.

Inspect the server's `/doc` schema first. Only literal-loopback HTTP with an explicit port is accepted; credentials and preflight: [setup](HOST_SETUP.md#setup). The [OpenCode 2 plugin API](https://opencode.ai/v2/docs/build/plugins) (`Plugin.define`) differs from v1: do not copy a v1 `experimental.chat.messages.transform` example. A plugin is optional (identity, presence, usage) when the HTTP server is reachable; without either, raw CLI/SQLite works.

## Failure and context policy

- Identity resolves through vendor labels and native bindings. A native-bound identity keeps presence/end events but gets no hook context; a second registration for that receiver is rejected (reuse the reported identity), and raw hook init cannot replace a concurrent native attachment.
- Deterministic Python, ten-second host timeout, 1 MiB stdin bound. Invalid input or workspace mismatch logs and fails open.
- Only supported injection events stage messages; others leave rows untouched. Hook payloads and tool results are never stored.
- SQLite serializes concurrent identity creation and staging. Restarts reuse the identity; expiry neither deletes audit nor replays.
- Messaging hooks change neither tool permissions nor tool results and never block completion.
- Presence renews on events; an idle host expires unless a listener keeps it, and direct messages wait for its next event.
- SDK acceptance and handling ACKs stay separate from stdout success; a crash after staging needs explicit recovery.

## Claude: bounded completion check alongside native delivery

For an attached Claude recipient, the host can configure `completion-check` as a process-local [Stop hook](https://code.claude.com/docs/en/hooks#stop):

```json
{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"'/absolute/path/agents-communication' completion-check - --workspace '/repo' --database '/db/communication.sqlite' --session 'DB_SESSION_ID'","timeout":10}]}]}}
```

Shell-quote the real paths and DB identity. Bind the native session and canonical workspace before the first Stop event, keep the listener running, and expose `inbox`. The command checks stdin `session_id` and `cwd` against the binding. Opt-in only.

- It lists at most 16 submitted, unexpired, unacknowledged IDs, without bodies; fetch one with `inbox({message:ID})`, then reply/ACK.
- `stop_hook_active:true` returns `{}`, so a blocked task cannot loop. Invalid bindings fail with a diagnostic and complete nothing.
- Read-only: no staging, resend, audit, permission changes, idle wake, provider recovery, or evidence of handling; the socket receipt stays write-only. Passive mail is ignored. Claude may show a Stop-hook error notice for a block; the event still records success.

## Identity context generations

Claude/Codex/Cursor emit binding context once at session start, Grok on the first supported post-tool event; flags are relative to host cwd, with `--database` omitted for the default DB. Later batches carry only new data. A host can supply a stable new `context_generation` after reset/compaction; repeats with no messages stay silent. No model turn starts, and elapsed time implies no reset.

Without an eligible message or new generation, hooks take a read-only path and skip the writer lock; creation, renewal, delivery and teardown stay transactional.

## Collaborator directory

Hooks add bounded peer changes: IDs, names, declared tasks, availability. Publish a short task and `available`, `busy` or `blocked` via `join`, `heartbeat` or `set_status` (default `unknown`). These are peer data, not capabilities.

- Hooks emit changes once per stored view; plain heartbeats stay silent; a new `context_generation` restores the directory too.
- `removedFromView` may mean pagination displaced a row: check `peers` before you infer departure. Follow `next`; `refresh` means a change may lie past the cached page. Views hold at most 16 rows, 3,000-byte row budget (one complete row always fits).
- Pi inserts directory notices without a turn; native recipients get changes with their next delivery; managed workers also get an initial directory. No roster change wakes an idle host; `peers` is the current lookup.

Pi `completionCheck: true`: after a real turn ends, the extension checks submitted, unacknowledged IDs and allows one recovery turn per work cycle (new action mail or user input starts a cycle; recovery does not). Passive mail never wakes; session/workspace changes cancel stale checks; it never acknowledges unfinished work.

Pi sends `strict:false` on its communication function descriptors for OpenAI Chat/Responses, which otherwise may make omitted optional routing fields required. It also projects top-level schema unions into plain object parameters for providers that reject root `oneOf`/`anyOf`/`allOf`; the canonical Python catalog still validates exact routing and payload rules. Other tools and schemas are unchanged. See [OpenAI function calling](https://developers.openai.com/api/docs/guides/function-calling).

## Custom bridge confirmation

Run `scripts/inbox-hook` (Windows: `.ps1`) from a host context event. Inject its stdout; empty output means no new messages.
For deferred confirmation, call `hook {"format":"json","deferConfirm":true}`.
Retain every offered message ID and `dispatchToken`.
Call `confirm_delivery` only after the host SDK accepts the context.
Call `complete` separately after handling; acceptance is not acknowledgement.
`host-hook` consumes actual host event JSON on stdin. It does not replace manual joining.
