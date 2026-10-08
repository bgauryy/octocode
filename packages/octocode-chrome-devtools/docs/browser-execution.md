# Combined browser flows and focused evidence

Load when a known flow needs multiple actions, waits, extraction, or frame scope. Why: combine a flow while preserving step evidence. Use one executor invocation, then query its saved evidence. Global URL search can still use the agent's search tool.

```bash
node "$CDP" run --port 9222 --target <id> --plan /absolute/plan.json
```

`run --json` or `run --plan -` (stdin) can supply the same JSON. Prefer a file for large plans; execution stages a private file rather than putting the plan in process arguments or environment. The capture saves the complete plan. `run --dry-run` validates without contacting Chrome. Unknown fields, invalid actions and unsupported operations fail before any step. Each step resolves its current target, emits progress, saves results and stops the flow on failure. Diagnose failures before trying another plan. The executor never repeats a failed mutation automatically.

```json
{
  "waitMs": 8000,
  "observe": {"network": true, "afterMs": 0},
  "steps": [
    {"op": "goto", "url": "https://example.com", "after": {"selector": "input[type=search]"}},
    {"op": "act", "role": "textbox", "name": "Search", "action": "fill", "value": "browser"},
    {"op": "act", "role": "button", "name": "Search", "action": "click", "after": {"text": "Results"}},
    {"op": "extract", "selector": "main a", "fields": ["text", "href"]}
  ]
}
```

Use the page's observed labels and selectors; this example is a plan shape, not a site contract.

| Operation | Fields and behavior |
|---|---|
| `goto` | Root `url`; waits for navigation commit and document readiness, including redirects. `after` adds task readiness. |
| `act` | `action` and explicit `ref`, `selector`, or exact `role` + `name`; trusted input and actionability checks; foregrounds the owning page, including isolated frames. Optional `value`, `key`, `toRef`, `toSelector`. |
| `wait` | Visible target or text `value`; no action. |
| `extract` | CSS `selector`; all matches saved. `fields`: `text`, `href`, `value`, `role`, `name`; default text + href. `name` reads aria-label/name attributes, not computed accessible name. Empty is failure unless `allowEmpty: true`. |
| `cdp` | Any installed `Domain.method`, optional object `params`; saves the full result. Runtime exceptions fail. |
| `readStream` | String or referenced `handle`, optional `size` (1–1048576, default 65536); saves every IO.read chunk through EOF as JSONL, then closes unless `close: false`. |
| `listen` | Unique `id`, `event` as Domain.event, optional `where` mapping JSON pointers in the payload to scalar equality tests; streams every event into JSONL, counts matches separately. |
| `waitEvent` | Earlier `listener` id, optional positive `count` (default 1); waits within the step readiness deadline. |
| `protocol` | Installed `domain`, optional command/event `member`; selected definitions and domain types saved. |

All steps accept `timeoutMs`, overriding `waitMs` (default 8000) for the whole step, including readiness, target polling and protocol requests. Individual protocol requests use plan `commandMs` (default `waitMs` or 8000), capped by the runner's `--timeout` (default 60000); each request is also capped by the remaining step deadline. Progress continues during a slow request. Pointer input uses direct trusted events; `fill` inserts the full value, while `type` sends key events without artificial typing delays. `after` accepts nonempty `text`, `selector`, and/or exact `url`. Actions verify requested text in their target realm; selectors must be visible. Document readiness alone does not prove SPA content is ready. `settleMs` defaults to zero; choose content conditions instead of fixed sleeps. `traceEvents: true` records input-event trust and timing without keys/data.

Optional `frame` scopes a step. Use `{"selector":"iframe[title='Payment']"}` for a single same-process frame owner, or `{"id":"<FRAME_TARGET>"}` / `{"url":"/payment"}` for an isolated iframe. URL matches are restricted to descendants of the selected tab and ambiguity fails. Inline frames use an isolated DOM world; page-script globals are unavailable there. Each inline step resolves its current frame/context. Navigate the root first, then scope child operations. Raw CDP commands still follow their domain's own scoping rules. `listen` with `frame.selector` is rejected during plan validation: same-process events cannot be isolated to that frame. Use an isolated frame id/URL or an explicit flattened session for scoped listeners.

`observe.network` attaches listeners before steps. `afterMs` is additional observation time after the flow, default zero. Completed, failed, redirected and pending requests remain in network artifacts. This observer captures metadata, not response bodies; use the body recipe or raw CDP for those. Traffic preceding attachment remains unobserved; iframe coverage is disclosed. All executor results are saved through digest-pinned artifact continuations.

## Adaptive navigation and advanced plans

`after.text` is a literal string, including `|`; selector readiness requires visibility and nonzero opacity. `session` scopes CDP, listeners, streams and DOM steps to a flattened session id, or a `$step`/`$event` reference to that id. It cannot combine with `frame`. Subscribe to `Target.attachedToTarget` before attaching or enabling auto-attach, then use its `/sessionId`. Child listeners filter that session's events. Inspect target ownership: detaching an auto-attached worker uses `Target.detachFromTarget` in its parent page session. Caller-created sessions remain caller-owned; explicitly detach when finished.

The agent owns decisions: inspect the current page, choose the control whose observed role/name or destination advances the goal, execute only the known transition, and verify its effect. After a menu, navigation or frame change, inspect the new state before choosing the next transition. Stop on ambiguous targets or missing authorization. A plan executes explicit steps; it does not infer user intent or choose an unseen destination.

CDP parameters can reference earlier results: `{"$step": 2, "pointer": "/result/objectId"}`. Event references use `{"$event":"trace","pointer":"/stream"}` after a matching event. These replace custom glue code for object handles, trace streams, console/network/worker events and chained protocol commands. Subscribe before triggering the event; inspect the installed schema for required enable commands. Stream artifacts disclose their observation boundary and preserve every event, including those that did not match the wait predicate.

A trace flow can use `listen Tracing.tracingComplete` → `cdp Tracing.start` with `transferMode: ReturnAsStream` → `cdp Tracing.end` → `waitEvent` → `readStream` using the event's `/stream`. Every chunk through EOF is saved in order. Decode base64-encoded data per chunk, otherwise UTF-8, then concatenate the bytes. Application-specific algorithms can still require custom logic. On a command timeout the flow stops and reports uncertain mutation outcome; inspect state before retrying.

## Query before paging

Typed CLI/MCP execution replies return direct `data`, a compact `flow`, or a capture manifest; progress logs stay in saved files. Read the first artifact page, then follow `next.artifacts` for remaining inventory rows. To filter inventory, start a fresh `query` on the manifest's `/artifacts` array; do not change filters on an existing cursor. `next.findings` reads findings when present, and `next.capture` includes all details. Reader results are direct `data` pages. Every package continuation is `{tool,query}`: call MCP with `{name:tool,arguments:query}`, or typed CLI `<tool> --input file --json` with that query as the file content.

For web content and arbitrary CDP text, use Octocode on the returned `search.paths`. Supply a task-specific literal; preserve the returned capture-search flags because artifacts are hidden and ignored. Run from the capture workspace. Inspect Octocode's live schema before constructing an unfamiliar query.

```sh
octocode localSearch '{"queries":[{"path":"<capture-directory>","matchString":"<task anchor>","regex":"literal","hidden":true,"noIgnore":true,"defaultExcludes":false}]}'
octocode localFetch '{"queries":[{"path":"<observed-result-file>","ranges":["<observed-start>-<observed-end>"]}]}'
```

Use indexed `query` for arrays, JSONL, HAR `/log/entries`, or a CDP array such as `/result/value`: filter before paging, choose a small page only when useful, and copy its digest-pinned next call unchanged. Use `artifact` for selected objects, text or binary sources. Local tools provide their own complete matches, exact line anchors, read leads and continuations; the browser package delegates that search rather than copying native tool logic.

```bash
node "$CDP" query --file /absolute/live-network.har --pointer /log/entries --where '[{"path":"/response/status","op":"gte","value":400}]' --limit 20
node "$CDP" query --file /absolute/events.jsonl --format jsonl --where '[{"path":"/type","op":"eq","value":"input"}]'
```

Predicates use JSON pointers with `eq`, `contains`, `gte`, `lte`, or `exists`. Multiple predicates are ANDed. `exists` takes a boolean; numeric comparisons accept a number or an integer string for large integers. Matching rows retain original source index and pointer. `--select '["/request/url","/response/status"]'` explicitly requests projection; original evidence remains in the source. Missing projected fields appear as null and can be distinguished in that source.

The first query builds a reusable offset index before pagination. In raw helper output, every matching row is reachable through `next.continue`; typed CLI/MCP maps it to `next.tool` and `next.query`. Oversized rows provide another executable continuation to their full value. A source digest pins continuations, and index/oversized row corruption fails. JSON envelopes and JSONL stream; selected JSON rows are parsed one at a time. The offset index is stored on disk. Memory follows the largest selected row, nesting and key size, rather than the whole array. Duplicate selected JSON paths fail as ambiguous. Each call hashes the source and index for integrity, so index reuse saves parsing/filtering, not all I/O. Arbitrary objects, raw text, binary data and original sources use `artifact-query.mjs`.

Next: a flow needs custom logic → [script patterns](script-patterns.md); a target or wait fails → [recovery](recovery.md).
