# RFC: `octocode-agent` — a standalone Octocode coding-agent CLI on Pi

- **Status:** Draft / for review
- **Author:** (bgauryy)
- **Date:** 2026-07-01
- **Package:** `packages/octocode-agent` (new) — npm `@octocodeai/agent`, bin `octocode-agent`
- **Related:** `packages/octocode-pi-extension` (reused as extension), `docs/PI/APPEND_SYSTEM.md` (prompt core)

---

## 1. Summary

Fork the bootstrap of Pi's `coding-agent` (`@earendil-works/pi-coding-agent`) into a new
first-party package, `octocode-agent`, that consumes Pi as a **library** rather than being
installed *into* a host `pi`. The new CLI:

1. Owns its app shell (`createAgentSessionRuntime` + `InteractiveMode`), so it controls
   startup chrome, tools, and the system prompt.
2. Ships **one authored system prompt** (seeded from `APPEND_SYSTEM.md`) instead of Pi's
   default prompt with our block **appended** after it.
3. Restricts the LLM to `read`, `bash`, `edit`, `write` (Pi builtins) **plus** two custom
   tools `web_search` and `web_fetch`. Pi's `grep`/`find`/`ls` are dropped — discovery goes
   through Octocode.
4. Reuses the existing `octocode-pi-extension` verbatim as an extension factory (awareness
   file-locks, skills discovery, `octocode-*` commands).

Distribution: `npx octocode-agent`. The existing `@octocodeai/pi-extension` package keeps
working unchanged for users who already run their own `pi`.

## 2. Motivation

Today `octocode-pi-extension` is a **Pi package**: the user installs it into a host `pi`
(`pi update @octocodeai/pi-extension`), and on every turn it **appends** our harness block
after Pi's full default system prompt via `before_agent_start`
(`src/index.js`: `systemPrompt: ${event.systemPrompt}\n\n${addendum}`).

Problems with the append model:
- **Two prompts, one contradiction surface.** The agent reads Pi's default persona/guidelines
  *and* our architect operating-model. They overlap and can conflict; the append sits *after*
  Pi's content, so Pi's framing is read first.
- **No control over Pi's defaults.** We can't drop Pi's extra tools, trim Pi's guidelines, or
  suppress startup chrome — we're a guest.
- **Longer prompt than necessary.** Pi's tool docs + guidelines + pi-documentation pointers
  ship on every turn even though our harness supersedes most of them.

Owning the shell (the `octoflow-cli` pattern, already proven in this org) fixes all three.

## 3. Goals / Non-goals

**Goals**
- A runnable `npx octocode-agent` TUI with a single, lean, authored system prompt.
- Tool surface = `read, bash, edit, write, web_search, web_fetch`.
- Zero-rewrite reuse of `octocode-pi-extension` (awareness, skills, commands).
- Keep the existing pi-package distribution working (backward compatible).

**Non-goals**
- Replacing the `octocode` research CLI (`npx octocode`) — that stays as-is; `octocode-agent`
  *drives* it via `bash`.
- Building our own agent loop / model layer — we use Pi's.
- Removing Pi's built-in **slash commands** (not cleanly possible — see §7).

## 4. Background — the two integration models (verified)

| | `octocode-pi-extension` (today) | `octocode-agent` (this RFC) |
|---|---|---|
| Role | Pi **package**, installed into host `pi` | Standalone **CLI** |
| Pi dependency | `octocode` CLI only | `@earendil-works/pi-{coding-agent,ai,tui}` as **libraries** |
| Owns app shell? | No (Pi does) | Yes (`createAgentSessionRuntime`, `InteractiveMode`) |
| System prompt | Append after Pi default | Single authored prompt (replace) |
| Tools | Whatever Pi enables | Explicit allowlist |
| Proof it works | — | `Octoflow/packages/octoflow-cli` (same pattern, shipping) |

Evidence: `@earendil-works/pi-coding-agent@0.79.4` public API (`dist/index.d.ts`,
`core/sdk.d.ts`, `core/system-prompt.js`, `core/extensions/types.d.ts`), and the working
`octoflow-cli` bootstrap.

## 5. Design

### 5.1 Package layout (mirrors `octoflow-cli`)

```
packages/octocode-agent/
  package.json            # bin: ./bin/octocode-agent.js
                          # deps: @earendil-works/pi-{coding-agent,ai,tui}@^0.79.4,
                          #       @sinclair/typebox, tsx, @octocodeai/pi-extension (workspace:*)
  bin/octocode-agent.js   # thin launcher → tsx src/index.ts (fork octoflow bin/factory.js;
                          #   drop factory-only --path / -q one-shot; keep -c/--continue, --version, --help)
  src/index.ts            # interactive entry (see 5.2)
  src/prompt.ts           # OCTOCODE_CORE_PROMPT (seeded from APPEND_SYSTEM.md)
  src/system-prompt.ts    # composeSystemPrompt() — the before_agent_start replacement (see 5.4)
  src/web-tools.ts        # defineTool web_search + web_fetch (see 5.5)
  src/octocode-context.ts # runtime block: octocode auth status + bundled CLI path
  tsconfig.json  README.md  LICENSE
```

Ship TypeScript + run via `tsx` (no build step), exactly as `octoflow-cli` does — the bin
resolves `tsx/cli` and spawns the entry.

### 5.2 Bootstrap (`src/index.ts`)

Forked from `octoflow-cli/src/index.ts`:

```ts
const parsed = parseArgs(process.argv.slice(2));            // --version/--help/-c handled here

const createRuntime: CreateAgentSessionRuntimeFactory = async ({ cwd, sessionManager, sessionStartEvent }) => {
  const services = await createAgentSessionServices({
    cwd,
    resourceLoaderOptions: {
      extensionFactories: [ createOctocodeAgentExtension(), octocodePiExtension ],
    },
  });

  services.settingsManager.applyOverrides({ quietStartup: true });   // remove startup chrome (in-memory)

  const created = await createAgentSessionFromServices({
    services, sessionManager, sessionStartEvent,
    tools: ['read', 'bash', 'edit', 'write'],                        // <-- allowlist, drops grep/find/ls
    customTools: createWebTools(),                                   // <-- web_search + web_fetch
  });
  return { ...created, services, diagnostics: services.diagnostics };
};

const runtime = await createAgentSessionRuntime(createRuntime, {
  cwd, agentDir: getAgentDir(),
  sessionManager: parsed.continue ? SessionManager.continueRecent(cwd) : SessionManager.create(cwd),
});
await new InteractiveMode(runtime, { /* initialMessages, verbose, … */ }).run();
```

`createOctocodeAgentExtension()` is the CLI-owned extension that registers the single
`before_agent_start` handler (§5.4). `octocodePiExtension` is the existing package's default
export (awareness + skills + `octocode-*` commands), with its own prompt-append **disabled**
(§5.6).

### 5.3 What Pi's default prompt actually provides (the research)

From `core/system-prompt.js` `buildSystemPrompt()`, the **default** prompt (when no
`customPrompt`) is:

```
You are an expert coding assistant operating inside pi, a coding agent harness. …
Available tools:
- read: <snippet>
- bash: <snippet>
- edit: <snippet>
- write: <snippet>
Guidelines:
- <tool-derived + "Be concise" + "Show file paths clearly">
Pi documentation (read only when the user asks about pi itself …): <paths>
<project_context> … AGENTS.md etc. … </project_context>
<skills> … formatSkillsForPrompt(skills) … </skills>
Current date: <date>
Current working directory: <cwd>
```

**Key finding — the `customPrompt` branch** (`system-prompt.js:19-41`): when a custom prompt is
supplied, Pi drops the default persona/tools/guidelines/pi-docs header and instead uses our
text, but **still auto-appends**:
- `<project_context>` from loaded context files (AGENTS.md, CLAUDE.md, …),
- the skills section (`formatSkillsForPrompt`, only if `read` is enabled — it is),
- `Current date:` and `Current working directory:`.

So of everything Pi injects, the agent's *operating knowledge* that our custom prompt must
carry itself is exactly **one thing our current `APPEND_SYSTEM.md` lacks: a concrete
"Available tools" list.** Everything else (project context, skills, date, cwd) is still
provided by Pi for free.

### 5.4 Minimal system prompt + the replacement mechanism

**`buildSystemPrompt` is not exported**, and there's no SDK option to set `customPrompt`
directly. But `before_agent_start` hands us `event.systemPromptOptions` (the
`BuildSystemPromptOptions` Pi used: `toolSnippets`, `selectedTools`, `contextFiles`, `skills`,
`cwd`) and the result contract is `{ systemPrompt?: string }` (replace, chained across
extensions). `formatSkillsForPrompt` **is** exported. So we reconstruct Pi's `customPrompt`
branch ourselves — this keeps project context / skills / date / cwd intact while replacing
only the persona/tools/guidelines:

```ts
// src/system-prompt.ts
import { formatSkillsForPrompt } from '@earendil-works/pi-coding-agent';
import { OCTOCODE_CORE_PROMPT } from './prompt.js';

export function composeSystemPrompt(opts /* = event.systemPromptOptions */): string {
  const tools = opts.selectedTools ?? ['read', 'bash', 'edit', 'write'];
  const visible = tools.filter((t) => opts.toolSnippets?.[t]);
  const toolsList = visible.length
    ? visible.map((t) => `- ${t}: ${opts.toolSnippets[t]}`).join('\n')
    : '(none)';

  let prompt = OCTOCODE_CORE_PROMPT
    + `\n\n<available_tools>\n${toolsList}\n`
    + `Custom tools may also be present (e.g. web_search, web_fetch); prefer Octocode for code discovery and web_* for the open web.\n</available_tools>`;

  for (const { path, content } of opts.contextFiles ?? [])
    prompt += `\n\n<project_instructions path="${path}">\n${content}\n</project_instructions>`;

  if (tools.includes('read') && opts.skills?.length)
    prompt += formatSkillsForPrompt(opts.skills);

  const cwd = opts.cwd.replace(/\\/g, '/');
  prompt += `\nCurrent date: ${today()}\nCurrent working directory: ${cwd}`;
  return prompt;
}
```

Wired in the CLI-owned extension:

```ts
pi.on('before_agent_start', (event) => ({ systemPrompt: composeSystemPrompt(event.systemPromptOptions) }));
```

**Result:** a single prompt = `APPEND_SYSTEM.md` core + concrete tool list + (Pi-provided)
project context, skills, date, cwd. No Pi default persona, no double framing.

**Prompt core changes** (`src/prompt.ts`, seeded from `APPEND_SYSTEM.md`):
- Keep all sections verbatim: `authority`, `operating_model`, `memory_and_reflection`,
  `tool_priority`, `skills`, `how_to_build`, `clean_code_architecture`,
  `contracts_and_data_flows`, `communication`, `delegation`, `safety`.
- Change the opening line to name the harness: *"You are the Octocode coding agent operating
  inside a Pi-based CLI."* (Pi's default said "operating inside pi"; we own that now.)
- The `<available_tools>` block is composed at runtime (above), not hard-coded, so tool
  snippets stay accurate to whatever Pi/version reports and to the enabled allowlist.

### 5.5 Web tools (`src/web-tools.ts`)

Port `Octoflow/packages/octoflow-tools/src/tools/{websearch.ts,web-url.ts}` from octoflow's
`defineAction` to Pi's `defineTool` (`ToolDefinition`: TypeBox `parameters`, `execute(id,
params, signal, onUpdate, ctx) → AgentToolResult`):

- `web_search` — DuckDuckGo HTML endpoint scrape (`html.duckduckgo.com/html/?q=`), returns
  `{title,url,snippet}[]`. No API key. `type: web|news`, `maxResults`. Failures return an
  `error` field, never throw. Uses `signal` for the abort/timeout.
- `web_fetch` — fetch a URL and return text/markdown. **Keep octoflow's SSRF guard verbatim**
  (`isPrivateHost` / `validateHttpUrl`: blocks loopback, RFC-1918, link-local, IPv4-mapped
  IPv6, IPv6 ULA/link-local; http/https only; optional host allowlist). This is a security
  control, not incidental — port it exactly.

TypeBox is already an octoflow dependency (`typebox@^1.1.38`); use `@sinclair/typebox`'s
`Type.Object({...})` for `parameters`.

### 5.6 Reusing `octocode-pi-extension` (one small change)

The existing default export `octocodePiExtension(pi)` registers: `resources_discover`
(skill paths), `before_agent_start` (**append**), `tool_call`/`tool_result` (awareness
file-locks), and the `octocode-status` / `octocode-setup` / `octocode-mcp-install` /
`octocode-skills-update` commands. All of that is reused as-is **except** the append handler,
which would double the prompt.

Parameterize the factory without breaking the current package:

```js
// octocode-pi-extension/src/index.js
export function createOctocodePiExtension({ promptMode = 'append' } = {}) {
  return function octocodePiExtension(pi) {
    /* … existing registrations … */
    if (promptMode === 'append') {
      pi.on('before_agent_start', /* existing append handler */);
    }
    /* awareness + commands + resources_discover unchanged */
  };
}
export default createOctocodePiExtension();   // zero-arg default = today's behavior (append)
```

`octocode-agent` imports `createOctocodePiExtension({ promptMode: 'off' })` and owns the
prompt itself. Existing pi-package users get the unchanged default export.

## 6. What Pi defaults we remove / override (verified levers)

| Pi default | Action | Lever |
|---|---|---|
| Tools `grep`, `find`, `ls` | **Remove** | `tools: ['read','bash','edit','write']` allowlist (`createAgentSessionFromServices`) |
| Default persona + guidelines + pi-doc pointers | **Replace** | `before_agent_start` → `{ systemPrompt: composeSystemPrompt(...) }` |
| Auto tool-docs block | **Rebuild lean** | reconstructed from `systemPromptOptions.toolSnippets` for the allowlisted tools only |
| Startup chrome (logo, keybinding cheatsheet, loaded-resources list) | **Remove** | `settingsManager.applyOverrides({ quietStartup: true })` (in-memory, non-persistent) |
| Project context / skills / date / cwd | **Keep** | Pi auto-appends them; we re-emit them in the rebuild |
| Providers/models | unchanged | `pi.registerProvider(...)` available if needed |
| **Built-in slash commands** (`/model`, `/login`, …) | **Cannot remove** | API exposes `getCommands()` + `registerCommand()` (add/override) but no `removeCommand` |
| Add web tools | **Add** | `customTools: createWebTools()` / `pi.registerTool(defineTool(...))` |

## 7. Best practices applied

- **Explicit tool docs.** With a custom prompt, Pi no longer auto-documents tools; an agent
  that can't see its tool list mis-calls or under-uses them. We compose the list from the
  live `toolSnippets` so it's always accurate to the enabled set and Pi version.
- **Stable content first (prompt caching).** The `OCTOCODE_CORE_PROMPT` is constant; the
  variable tail (context files, skills, date, cwd) goes last — matching Pi's own ordering and
  keeping the cacheable prefix large.
- **Lean over verbose.** Dropping `grep/find/ls` shrinks the tool block; dropping Pi's
  persona/guidelines/pi-doc pointers removes ~18 lines the harness already supersedes.
- **Security control ported, not reinvented.** `web_fetch` keeps the SSRF allow/deny logic
  intact.
- **Backward compatibility.** The extension's default export is unchanged; only an
  opt-in parameter changes behavior for the new CLI.

## 8. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Coupling to Pi's `customPrompt`-branch layout (we reconstruct it) | Keep the reconstruction ~15 lines; pin `@earendil-works/pi-*@^0.79.4` (octoflow's known-good baseline); a Pi bump is a deliberate, tested step. |
| Dropping `grep/find/ls` degrades discovery if octocode/MCP unavailable | `bash` remains — `rg`/`find` still reachable via shell; prompt already steers to octocode with shell as fallback. |
| Custom prompt weakens tool-calling | Tool list is composed from real `toolSnippets`; smoke-test `read/edit/write/bash` + `web_*` calls before shipping. |
| We own Pi version churn (0.79→0.80 moved SDK APIs) | Single pinned version; upgrade behind a test. |
| `web_search` scraping DuckDuckGo is brittle | Failures return `{error}`, never throw; agent falls back to `web_fetch` of a known URL or octocode. |

## 9. Rollout / build order

1. Scaffold `packages/octocode-agent` (`package.json`, `tsconfig`, LICENSE) — copy octoflow, rename.
2. `bin/octocode-agent.js` + `src/index.ts` (strip factory-only flags; keep `-c/--continue`, `--version`, `--help`).
3. Parameterize `octocode-pi-extension` factory (`promptMode`); default export unchanged.
4. `src/prompt.ts` (seed from `APPEND_SYSTEM.md`, adjust opening line) + `src/system-prompt.ts` (`composeSystemPrompt`) + CLI-owned `before_agent_start`.
5. `src/web-tools.ts` (port `web_search` + SSRF-guarded `web_fetch` to `defineTool`).
6. `src/octocode-context.ts` (auth status + bundled CLI path note).
7. Smoke test: launches with quiet startup; `getActiveTools()` == `[read,bash,edit,write,web_search,web_fetch]`; single prompt (no Pi persona, project context + skills present); awareness locks + `octocode-*` commands work.

## 10. Open questions

- Ship as `tsx`-run source (like octoflow) or precompile to `dist/`? (Default: `tsx`, simplest.)
- Bundle the `octocode` CLI into this package (as `octocode-pi-extension` does) or rely on the
  `octocode` dependency the extension already carries? (Default: reuse the extension's bundled CLI.)
- Do we want a one-shot / print mode (`-q`)? Pi exposes `runPrintMode`; out of scope for v1.
