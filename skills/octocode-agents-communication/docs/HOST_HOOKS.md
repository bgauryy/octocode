# Native host hooks and OpenCode

No user-level host settings are modified. Native shell hooks belong in the host's
configuration; portable SKILL.md frontmatter is not a universal hook installer.

## Event contracts

| Host | Identity/presence | Inbox context | Unsupported for delivery |
| --- | --- | --- | --- |
| Cursor | `sessionStart`, `beforeSubmitPrompt`, tool events; `sessionEnd` leaves | `postToolUse` / `postToolUseFailure`: `additional_context` | `beforeSubmitPrompt` only validates; don't stage there |
| Grok Build | `SessionStart`, `UserPromptSubmit`, tool events; `SessionEnd` leaves | `PostToolUse` / `PostToolUseFailure`: `hookSpecificOutput.additionalContext` | Session-start output and allowing prompt-hook output are discarded |
| OpenCode | Existing session ID via server/SDK | `POST /session/:id/message` with `noReply:true` in the documented server contract | A shell hook is not required when the owning server is reachable |

Cursor's [official hooks reference](https://cursor.com/docs/hooks) defines the
input IDs, native configuration, post-tool context and session lifecycle. SessionStart
is asynchronous and injects initial identity context, so it is not used to consume
peer messages. Native Cursor skill hook frontmatter is not assumed to execute.
Executable names alone are not sufficient vendor detection (Grok also installs an
`agent` command). The Cursor adapter is contract-fixture-tested, not live-model verified.

Grok's [upstream hook contract](https://github.com/xai-org/grok-build/blob/f0e3be1100ef5252488e3be8bb0e91cf68d8c305/crates/codegen/xai-grok-pager/docs/user-guide/10-hooks.md)
defines camelCase payload keys, PascalCase compatibility events, project hook
locations and post-tool output. Installed Grok 1.0.41 discovers project hooks only
when `inspect --json` identifies a project root: the live fixture needs a Git
repository, not just a temporary directory containing `.grok/hooks/`. Check
`projectRoot`, `projectTrusted` and `hooks` before relying on injection. Its 10,000-character context cap is why the adapter
switches to full-message DB references (ID and sender only) above an 8,000-byte
envelope. Items that still exceed 9,000 bytes return to `ready` before any output
and arrive with a later event, so a hook never strands a staged row. It does not
silently trim a body or automatically acknowledge it. PreToolUse injection exists,
but can be dropped when a tool is denied; post-tool delivery avoids that coupling.
Build `4220f3b224a6` accepts `--no-auto-update` only under `grok agent leader`,
not on top-level `grok` or `grok inspect`. The hook probe uses the installed help
contract and omits that obsolete top-level flag.

## OpenCode: direct API before hooks

The [server reference](https://opencode.ai/docs/server/) exposes messages and
`noReply`. In [the inspected implementation](https://github.com/anomalyco/opencode/blob/16c56fe5ecc3305028d1f0a9cff5806e51c9d480/packages/opencode/src/session/prompt.ts),
`createUserMessage` runs before the `noReply` early return; only the other branch
enters the model loop. This provides the same passive-delivery concept as Codex
injection. Keep the DB stage/confirm/complete protocol around that API. Never send peer
text as a `system` override. A receiving agent still uses the communication CLI/DB for replies.

Before integrating a server, inspect its `/doc` schema. This adapter accepts only
literal-loopback HTTP with an explicit port; [setup](../README.md#connect-a-native-recipient)
covers credentials scoped to the endpoint and workspace/status preflight.
The [OpenCode 2 plugin API](https://opencode.ai/v2/docs/build/plugins) differs from
v1 plugin hooks: it uses `Plugin.define` and domain APIs. Do not copy a v1
`experimental.chat.messages.transform` example into v2. A plugin is useful to
manage identity, presence and usage automatically; it is optional for message
transport when the owning HTTP server is available. With neither surface, the
same raw CLI/SQLite workflow already works.

## Failure and context policy

- Hooks resolve identity through both vendor labels and native transport bindings.
  A native-bound identity keeps lifecycle presence/end events but produces no hook
  context. Native attachment rejects a second registration for that receiver;
  reuse the reported DB identity. Raw hook initialization cannot replace a native
  attachment created concurrently.
- Hooks execute deterministic Rust, with a ten-second host timeout and a 1 MiB
  stdin bound. Invalid input/workspace mismatch logs diagnostics and fails open.
- Only supported injection events stage messages. Passive/ignored events leave
  inbox rows untouched. Full hook payloads and tool results are never stored.
- Concurrent hooks serialize identity creation and delivery staging in SQLite.
  A restart reuses the identity; expiry does not delete audit or replay deliveries.
- Cursor/Grok delivery hooks do not change tool permissions, substitute tool
  output, or block completion. The optional Claude completion check below is a
  separate read-only guard, not another delivery adapter.
- Presence renews on events. An idle host expires unless a supervised listener
  maintains presence; direct messages can still wait for its next event.
- SDK acceptance and processing acknowledgements remain separate from stdout
  success. A host crash after staging requires explicit recovery, as before.

## Claude: bounded completion check alongside native delivery

For an already attached Claude recipient, the host can configure the Rust
`completion-check` command as a process-local [Stop hook](https://code.claude.com/docs/en/hooks#stop):

```json
{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"'/absolute/path/agents-communication' completion-check - --workspace '/repo' --database '/db/communication.sqlite' --session 'DB_SESSION_ID'","timeout":10}]}]}}
```

Substitute and shell-quote the real paths and DB identity. Bind the native session
and canonical workspace before the first Stop event, keep its listener running,
and expose the `inbox` tool for selective recovery. The command reads native event
JSON from stdin and checks `session_id` and `cwd` against that binding. This is an
opt-in host configuration; installing the portable skill does not install hooks.

Only already-submitted, unexpired, unacknowledged deliveries qualify. The response
lists at most 16 IDs without message bodies. The agent handles known context or
fetches a missing body with `inbox({message:ID})`, then replies/ACKs normally.
`stop_hook_active:true` returns `{}`, preventing a repeated blocking loop when
work cannot finish. Invalid bindings fail with a diagnostic; they do not completion work.

The check neither stages nor resends context, writes audit state, nor grants
permissions. It does not wake an idle host, recover provider failures, guarantee
handling, or strengthen the socket's write-only receipt. Queued passive mail is
ignored. Claude may display a Stop-hook error notification for a blocking
decision; the native event still records the hook's successful exit and recovery.

## Identity context generations

Cursor emits its binding once at `sessionStart`; Grok emits it on the first supported
post-tool context event. The binding names CLI flags relative to the host's working
directory and omits `--database` for the default DB. New peer batches contain only their new data, not another
copy of the binding. Hosts that reset or compact away binding instructions must
supply a new stable `context_generation` string on the next supported event. Repeated
events with the same generation remain silent when no message arrived. This adapter
does not start a model turn or infer a context reset from elapsed time.

Fresh identities with no eligible message or new context generation use a
read-only path. Creation, renewal, delivery and teardown retain their transactional
checks. Native-bound identities stay silent. Empty hooks therefore avoid taking
the database writer lock; hooks that need mutations can still wait or fail softly.

## Collaborator directory

Hooks add bounded peer changes alongside message context: exact IDs, names, declared
tasks, and availability. Publish a short task and `available`, `busy`, or `blocked`
status through `join`, `heartbeat`, or `entity set session`; the default `unknown`
does not claim availability. Names and declarations are peer data, not capabilities.

Cursor/Grok context events and raw hooks emit changes once per stored view; ordinary
heartbeats stay silent. A new `context_generation` restores Cursor/Grok identity and
the directory after compaction. `removedFromView` can mean pagination displaced an
entry, so check `peers` before inferring departure. Follow `next`; `refresh` means a
change may lie beyond the cached page. Views contain at most 16 rows with a 3,000-byte
row budget (one complete row always makes progress).

Pi inserts directory-only notices without requesting a turn. Native recipients get
changes with their next message delivery; managed workers also receive an initial
directory. No roster update wakes an idle native host. Directory injection is a
best-effort convenience; `peers` remains the current lookup when choosing a recipient.

Pi bindings may set `completionCheck: true`. After an actual agent turn ends, the extension checks submitted, unacknowledged IDs and permits one recovery turn per work cycle. New action mail or user input starts a new cycle; its own recovery does not. Queued passive mail never wakes an idle agent. Session/workspace changes cancel stale checks. This is opt-in, read-only, and never acknowledges unfinished work.

Pi explicitly sends `strict:false` for its communication function descriptors on OpenAI Chat/Responses requests. This preserves optional routing fields: Responses may otherwise normalize omitted `strict` into required fields. Other tools and canonical schemas stay unchanged; Rust rejects invalid arguments. See [OpenAI function calling](https://developers.openai.com/api/docs/guides/function-calling).
