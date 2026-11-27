Load when measuring or reducing the context an MCP server costs, choosing how many tools to expose, or verifying that a wording edit kept behavior.

# Efficient MCP context

Optimize successful work per loaded context, not tool count or character count alone.

## Measure the served surface

Run [mcp-surface.mjs](../scripts/mcp-surface.mjs) from the directory the server expects; `--help` lists options, outputs and exit codes.

```bash
node <skill-folder>/scripts/mcp-surface.mjs capture --cmd '<server command>' --out <round>/before.json
node <skill-folder>/scripts/mcp-surface.mjs size <round>/before.json
```

- `capture` detects the handshake (`server/discover` for revision 2026-07-28 and later, else `initialize`), writes that result and every `tools/list` page to a new file and stops the server. A server that speaks both is captured as `discover`: pass `--protocol 2025-11-25` to capture what it serves older clients, and compare captures of the same handshake; exit 1 means no capture: the printed error code (`serverExited`, `rpcError`, `protocolError`, `timeout`) and the server's `stderr` say why. Freeze the build for each comparison and keep the raw captures. Inspect the host's projected tool definitions separately: they can differ from the served list.
- `size` reports UTF-8 bytes per tool and `totals` (`toolBytes`, `instructionsBytes`, `surfaceBytes`, `schemaProseBytes`). Compare the same profiles, authorization and lifecycle states; separate variable identity/path metadata from static guidance so it cannot distort savings.
- Report bytes as bytes; token and cost claims follow [context metrics](context-budget.md#context-budget). A smaller definition is not evidence of better task success.

## Remove duplication, retain decisions

- Give each rule one [owner](tool-descriptions.md#layer-owners). Remove repeated routing, boilerplate and explanations already carried by constraints. Keep purpose that names cannot convey: `wholeWord` may still need “Use for identifiers.”
- Let types, required fields, enums and branch constraints express structure. Field notes explain non-obvious values, units, omission/default behavior and ID scope; do not recite every allowed value. Keep distinctions such as acknowledgement versus completion, timeout versus failure, and record ID versus message ID when the runtime has them.
- Schemas come from [one definition](tool-descriptions.md#unified-tool-contract), but shared source definitions do not guarantee smaller served schemas: adapters may inline them. Factor references or unions only when receiving hosts support them and measured exposure improves; preserve branch validation and execution-time checks.
- Prefer clear language to cryptic aliases, abbreviated fields or a second schema hidden in prose. Preserve evidence and executable continuations; a context target never justifies truncation or weaker validation.

## Choose a small, coherent tool set

- Merge aliases and overlapping operations when their purpose, lifecycle and authority agree. Related actions may share a typed family, for example `{type:"read",input:{...}}`, with exact fields per variant. Keep unrelated jobs separate; avoid an unrestricted `execute` tool or one object full of optional fields.
- Tool-level annotations and approval policies must remain accurate for every variant. Split a family when combining reads and mutations obscures permissions or recovery.
- Tool count has a measured cost: Anthropic reports selection degrading past 30–50 available tools and OpenAI suggests fewer than 20 at the start of a turn; apply the figure for your provider, and below it defer only tools that a trace shows unused in most runs. Past that, defer rare tools behind the host's tool search and keep the three to five most used loaded (Anthropic measured about 85% fewer definition tokens and higher accuracy on its own evaluations). Deferral adds a search step and can miss: third-party tests over thousands of tools report retrieval misses, so write deferred names and descriptions for search and test discovery of each. With under about ten small tools, load them all.
- Use supported profiles or scopes to expose only relevant operations. Descriptions and instructions must not promise hidden capabilities. Bootstrap tools and changing catalogs require the negotiated revision and client to support that discovery model; configuration files alone do not prove active hooks or identity binding ([MCP wire](tool-errors.md#mcp-wire)).

## Verify retained behavior

- For wording-only edits, capture after the edit and run `node <skill-folder>/scripts/mcp-surface.mjs diff <round>/before.json <round>/after.json`. Exit 0 means names, requiredness, defaults, bounds, enums, types, composition and annotations are unchanged; exit 2 means invalid input (arguments, an unreadable or malformed capture, duplicate tool names); exit 1 lists each `changes[]` row (`tool`, `path`, `kind`) to revert or justify; `prose.tools` names the tools whose wording changed. Check affected CLI help and host projections as well as MCP calls.
- For changed tool surfaces or substantial rewrites, use [cold interviews and live replay](interview.md): selection, request/result handling, invalid combinations, hidden capabilities, retries, empty pages, continuations and untrusted content. Execute proposed calls; valid syntax or self-reported confidence is not enough.
- Keep reductions that preserve required decisions and recovery. Report measured size and observed outcomes separately; use [sealed-test](eval-cases.md#splits-and-exposure) evaluation for comparative reliability or cost-per-success claims. Do not turn a few successful checks into a general model-performance claim.

## Sources

- [Anthropic: advanced tool use](https://www.anthropic.com/engineering/advanced-tool-use) (tool search and input examples; internal measurements) and [OpenAI: function calling](https://developers.openai.com/api/docs/guides/function-calling)
