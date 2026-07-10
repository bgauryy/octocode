# Pi Builtin Overrides — Why and How

Octocode’s Pi extension does not leave Pi’s default coding tools alone. It
**removes** weak read/search builtins and **replaces** the mutation builtins
(`edit`, `write`, `bash`) with Octocode implementations.

This doc is for:

- **Users / agents** — what changed in day-to-day tool use, and how to work with it
- **Developers** — why we did it, where the code lives, and how to extend it

Related: [TOOLS.md](./TOOLS.md) (full tool reference), `/octocode-status` and
`/octocode-harness` (live inventory).

---

## Why we override

Pi ships a small, capable default tool set (`read`, `grep`, `find`, `ls`,
`edit`, `write`, `bash`). That is fine for a generic coding agent. Octocode
needs a **stronger, consistent** local contract:

1. **One research stack** — Octocode already has `localGetFileContent`,
   `localSearchCode`, `localFindFiles`, `localViewStructure` (plus GitHub/LSP).
   Keeping Pi’s `read`/`grep`/`find`/`ls` next to them duplicates guidance,
   splits agent habits, and skips Octocode pagination/minify/AST modes.
2. **Safer mutations** — Pi’s stock `write`/`bash` did not share Octocode’s
   path-guard (cwd / home / OS temp / `ALLOWED_PATHS`). Agents could create or
   redirect into paths outside the intended roots while `edit` was already
   guarded.
3. **Faster, clearer edits** — the previous edit preview used an O(N·M) LCS
   line diff. On mid-size files that cost hundreds of milliseconds *per edit*
   (and omitted diffs above ~6k lines). Myers line diff brings that to
   sub‑millisecond for typical agent edits and keeps diffs available on larger
   files.
4. **Stale-read discipline** — Octocode `edit` records freshness from
   `localGetFileContent` (and from Octocode `write`) so concurrent or external
   changes fail loudly instead of silently clobbering work.

We did **not** override tools to be “different for its own sake.” We overrode
them so every structured read/search/mutation path speaks Octocode’s rules.

---

## Matrix (what happens to each Pi builtin)

| Pi builtin | Octocode behavior | Mechanism |
|---|---|---|
| `read` | **Removed** | Prefer `localGetFileContent` |
| `grep` | **Removed** | Prefer `localSearchCode` |
| `find` | **Removed** | Prefer `localFindFiles` |
| `ls` | **Removed** | Prefer `localViewStructure` |
| `edit` | **Replaced** | Same name; Octocode implementation |
| `write` | **Replaced** | Same name; path-guard + read-state |
| `bash` | **Replaced** | Same name; path-guard on write targets |

There is **no passthrough** Pi mutation builtin left. Live confirmation:

```text
/octocode-status
/octocode-harness
```

Look for `overridden: edit, write, bash` and `removed: read, grep, find, ls`.

---

## For users and agents

### What to use

| Job | Use |
|---|---|
| Orient a tree | `localViewStructure` |
| Search code | `localSearchCode` |
| Read a file / range | `localGetFileContent` |
| Surgical change | `edit` (exact `oldText` from a fresh read) |
| New file / full rewrite | `write` |
| Git, builds, tests, bulk `sed` | `bash` |

### Practical rules

- Prefer **`edit` / `write`** over shell redirection for ordinary file work.
- Before `edit`, re-read with `localGetFileContent` when the file may have
  changed (or after another agent touched it).
- `write` overwrites without an `oldText` match — intentional full replace only.
- `bash` still runs real shell commands. Redirects (`>`, `>>`), `tee`, and
  `cp`/`mv` destinations must stay inside allowed roots. A small set of
  catastrophic patterns is blocked. This is **not** a full sandbox — treat
  shell as powerful.
- Expand allowed roots with `ALLOWED_PATHS` in `~/.octocode/.env` (or project
  Octocode env) when you need to write outside cwd/home/temp.

### Path-guard roots

Allowed write targets for Octocode `edit` / `write` / guarded `bash` paths:

- current working directory
- home directory
- OS temp directory
- every entry in `ALLOWED_PATHS`

### Optional: native edit diffs

Edit previews use a fast JS Myers diff by default. To force the engine native
binding (usually unnecessary):

```bash
export OCTOCODE_EDIT_NATIVE_DIFF=1
```

---

## For developers

### Design choices

| Choice | Rationale |
|---|---|
| Same-name `registerTool` for `edit`/`write`/`bash` | Pi replaces the builtin; agents keep familiar names |
| `setActiveTools` filter for `read`/`grep`/`find`/`ls` | Remove weak duplicates; force Octocode locals |
| Re-assert disable on load, after registration, and `session_start` | Later `setActiveTools` must not silently re-enable weak builtins |
| Path-guard shared module | One access model for edit, write, bash write-targets, chrome `scriptFile` |
| JS Myers default; native opt-in | Measured: JS Myers ~0.3 ms @ 3k lines; debug napi was slower across the boundary |
| Bash guard = redirects / tee / cp|mv + blocklist | Keep git/builds working; close the obvious escape hatch next to guarded `write` |

### Code map

| Concern | Location |
|---|---|
| Disabled / overridden name lists | `src/constants.ts` (`DISABLED_BUILTIN_TOOL_NAMES`, `OVERRIDDEN_BUILTIN_TOOL_NAMES`) |
| Disable + status/harness wiring | `src/index.ts` (`disableBuiltinTools`, `formatStatus`, `listExtensionHarness`) |
| Custom `edit` + Myers / native opt-in | `src/tools/edit-tool.ts` |
| Custom `write` | `src/tools/write-tool.ts` |
| Custom `bash` + target extraction | `src/tools/bash-tool.ts` |
| Path-guard | `src/tools/path-guard.ts` |
| Native `computeLineDiff` (engine) | `@octocodeai/octocode-engine` → `computeLineDiff` |
| Agent-facing routing copy | `src/prompts/sections/tools.md` |

### Pi API used

- **Override:** `pi.registerTool({ name: "edit" | "write" | "bash", ... })` — same
  name as the builtin replaces it (Pi extension contract).
- **Remove:** `pi.getActiveTools()` / `pi.setActiveTools(names)` — drop
  `read`/`grep`/`find`/`ls` from the active set.

### Tests to keep green

- `tests/package.test.ts` — override labels, disable matrix, harness text, write path-guard
- `tests/myers-diff.test.ts` — Myers correctness + perf gate vs old LCS cliff
- `tests/bash-tool.test.ts` — redirect extraction + outside-root block

### Extending overrides

1. Prefer same-name override when replacing a Pi builtin the LLM already knows.
2. Put shared FS policy in `path-guard.ts`, not ad-hoc checks.
3. Update `OVERRIDDEN_BUILTIN_TOOL_NAMES` / `DISABLED_BUILTIN_TOOL_NAMES`,
   `/octocode-status` copy, this doc, and `TOOLS.md` in the same change.
4. Add a focused test that fails if the builtin comes back unguarded.

### What we deliberately did not do

- **Full bash sandbox / seccomp** — high blast radius; git and builds need a real shell.
- **Default native edit diffs** — JS Myers already beats the old LCS by orders of
  magnitude; native remains available for shared engine callers and opt-in.
- **Re-enabling Pi `read` as an audited wrapper** — we want `localGetFileContent`
  so edit stale-checks stay wired.

---

## Measured impact (edit diffs)

On a mid-size single-line change (re-measured locally):

| Lines | Old LCS (×2 passes) | Myers artifacts |
|---:|---:|---:|
| ~1k | ~44 ms | ~0.12 ms |
| ~3k | ~430 ms | ~0.37 ms |
| ~10k | omitted (>6k cap) | full diff returned |

Exact `oldText` matching was never the bottleneck; the O(N·M) preview was.

---

## See also

- [TOOLS.md](./TOOLS.md) — parameters and routing for every tool
- [MEMORY_AGENT_FLOW.md](./MEMORY_AGENT_FLOW.md) — Awareness coordination around edits
- Pi upstream: same-name `registerTool` override pattern in
  `@earendil-works/pi-coding-agent` extension docs
