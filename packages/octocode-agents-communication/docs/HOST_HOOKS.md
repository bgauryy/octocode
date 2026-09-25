# Native host hooks and OpenCode

Reviewed September 25, 2026 using the requested `octocode-skills` workflow.
No user-level host settings are modified. Native shell hooks belong in the host's
configuration; portable SKILL.md frontmatter is not a universal hook installer.
The skill remains one instruction file under 50 lines and the coordination runtime is Rust.

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
The installed `agent` command here belongs to Grok, not Cursor; executable names
alone are not sufficient vendor detection. Cursor's CLI is unavailable in this
checkout's PATH, so its adapter is contract-fixture-tested, not live-model verified.

Grok's [upstream hook contract](https://github.com/xai-org/grok-build/blob/f0e3be1100ef5252488e3be8bb0e91cf68d8c305/crates/codegen/xai-grok-pager/docs/user-guide/10-hooks.md)
defines camelCase payload keys, PascalCase compatibility events, project hook
locations and post-tool output. Installed Grok 1.0.41 discovers project hooks only
when `inspect --json` identifies a project root: the live fixture needs a Git
repository, not just a temporary directory containing `.grok/hooks/`. Check
`projectRoot`, `projectTrusted` and `hooks` before relying on injection. Its 10,000-character context cap is why the adapter
switches to full-message DB references above an 8,000-byte envelope. It does not
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
injection. Keep the DB stage/confirm/ack protocol around that API. Never send peer
text as a `system` override. A receiving agent still uses the communication CLI/DB for replies.

See the [OpenCode evaluation](OPENCODE_EVALUATION.md) for live and fixture evidence.
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
work cannot finish. Invalid bindings fail with a diagnostic; they do not ACK work.

The check neither stages nor resends context, writes audit state, nor grants
permissions. It does not wake an idle host, recover provider failures, guarantee
handling, or strengthen the socket's write-only receipt. Queued passive mail is
ignored. Claude may display a Stop-hook error notification for a blocking
decision; the native event still records the hook's successful exit and recovery.
The six-agent recovery evidence is tracked in [context optimization](CONTEXT_PROFILES.md).

## Verification

`node --test tests/host-hooks.test.mjs` exercises both envelopes against the real
bundled Rust binary: identity reuse, lifecycle, ignored events, concurrent delivery,
workspace isolation, the Grok cap and timeout-bearing configuration previews.
`node scripts/grok-hook-poc.mjs` is the opt-in live Grok test in a temporary project,
using a fresh host session and a DB-only receipt/reply/ack exchange. Its latest report is
`out/grok-hook-poc.json`; timestamped harness, result and output logs are retained
under `.octocode/benchmarks/communication-grok-hooks/results/` at the repository root.
The test uses a process-local trust override only for its own generated fixture.

Live Grok Build **1.0.41 (`4220f3b224a6`)**, model **grok-4.7-build-fast**, passed in **30.78 s**
for the complete model/tool/reply/ack flow. The nonce appeared only in the committed
DB message, not the model task. The receiver stored one exact reply and one handling
ack; audit recorded one staged/submitted offer. See
[the receipt](../out/grok-hook-poc.json). This is one acceptance run, not a latency
benchmark. A previous attempt failed because the temporary folder had no Git
project root; the test now verifies hook discovery before starting inference.
This verification covers `PostToolUse` after an explicit initial tool call, **not
an idle wake**. The probe reads the CLI-served skill's `instructions` field once
(6,465 bytes for this run), and replies with `replyTo` so the DB resolves the sender.
The four-turn result reported 19,038 input tokens, 30,464 cached-input tokens and
3,139 output tokens; these are vendor result counters, not a single-context size
or a controlled token-savings comparison. The isolated project had no project
instructions, but inspection still discovered 41 user skills, three agents and
one MCP server. A Bash allowlist does not prove those catalogs were excluded from
the model context. Use the native API profile when explicit discovery controls
are required; this fallback probe does not claim a minimal host prompt.

The skill's standard validator passes. The repository-specific skill reviewer has
no missing-file or unused-file errors; four research-skill lobby convention errors
and its extra-README warning remain explicitly documented in
`.octocode/octocode-skills/agents-communication-review.md`. They are not reported
as a clean review pass: the standalone Rust runtime and one-file user constraint
take precedence over inserting unrelated research-tool boilerplate.

Recorded hook-probe checks: 32 CLI/process tests and 15 Rust contracts passed, including Python
DB-only conformance. The CLI files run serially to reduce observed fake-vendor
startup flakiness; the concurrent-hook and lock tests still explicitly exercise
parallel processes. Clippy, formatting, standalone skill validation and packaging
passed. The release skill archive was rebuilt for macOS ARM64.
The later four-vendor service validation is recorded in [Grok integration](GROK_INTEGRATION.md).

## Identity context generations

Cursor emits its binding once at `sessionStart`; Grok emits it on the first supported
post-tool context event. New peer batches contain only their new data, not another
copy of the binding. Hosts that reset or compact away binding instructions must
supply a new stable `context_generation` string on the next supported event. Repeated
events with the same generation remain silent when no message arrived. This adapter
does not start a model turn or infer a context reset from elapsed time.

Fresh identities with no eligible message or new context generation use a
read-only path. Creation, renewal, delivery and teardown retain their transactional
checks. Native-bound identities stay silent. Empty hooks therefore avoid taking
the database writer lock; hooks that need mutations can still wait or fail softly.
See the [matched hook benchmark](HOOK_EVALUATION.md) for contention measurements.
