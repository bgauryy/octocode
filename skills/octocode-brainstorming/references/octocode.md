# Octocode MCP and CLI

Load this when a task needs Octocode setup, transport choice, authentication, or command examples.

## Choose Transport

Use Octocode MCP tools directly when the host exposes them, such as `localSearchCode`, `ghSearchCode`, `ghGetFileContent`, `npmSearch`, `lspGetSemantics`, or `localBinaryInspect`. Read the tool description and input schema before calling.

When MCP tools are not exposed, prefer the CLI with `npx octocode`. Read live help before relying on flags, and read `npx octocode tools <name> --scheme` before raw tool calls.

## MCP Install

Configure the MCP server as:

```json
"octocode": {
  "command": "npx",
  "type": "stdio",
  "args": [
    "@octocodeai/mcp@latest"
  ]
}
```

Restart the host/editor after changing MCP configuration.

## CLI Usage

Run commands as `npx octocode <command>`.

Useful probes:

```bash
npx octocode --help
npx octocode auth status --json
npx octocode tools
npx octocode tools <name> --scheme
```

Use `npx octocode auth login` when GitHub or private data requires authentication. If neither MCP nor CLI is available, continue only with clearly degraded confidence or ask the user to enable one.

## Research Algorithm — local/external (distilled from docs/OCTOCODE_RESEARCH_MANIFEST.md, verified 2026-07-07)

**Route by what you already hold — never a fixed pipeline:**

- Nothing (unfamiliar code) → tree view + count-matches-per-file hotspot map, then re-enter the router with what you learned.
- A concept (words only) → synonym-regex text search (e.g. `halfLife|half_life|HALF_LIFE`), then a symbols view of the top file.
- An identifier → LSP workspaceSymbol, then callers/callees (callables) or references (everything else). Skip grep for locating.
- A code shape → structural AST search. A pattern must match a COMPLETE node (body, return type); use a rule (`kind`/`has`/`inside`/`not`, `stopBy: end`) for partial or relational matches; `foo($$$A)` does not match `x.foo($$$A)`.
- A package name → node_modules FIRST (`excludeDir: []` required, or the default exclusion skips it) — the installed version is ground truth, not GitHub's default branch; npm lookup only finds the source repo.
- A "why"/history question → PR search (keywords + match:title + concise) and commit history on the path.
- A binary/archive → binary inspect (list before extract; strings for leads).

**Reads:** matchString first — it returns merged slices plus `matchRanges[]` line anchors that feed LSP `lineHint` directly; line ranges second; fullContent last (small files only).
Map a file with `minify:"symbols"` (~10x smaller; constants keep values; the line gutter is an anchor sheet).
Quote/diff/edit only from `minify:"none"` — standard mode is lossy (rewrites quotes, strips comments).

**Local↔external gate:** workspace code → local tools; installed dependency → node_modules IS local; previously materialized repo → its localPath.
Only then go external — and materialize back to local (clone or directory fetch; depths file/tree/repo) once you need AST, LSP, multi-file regex, or a 3rd+ read into the same remote area.
After one bridge call, the full local loop (AST + LSP + matchString) runs on the returned localPath.

**Evidence discipline (non-negotiable):**

- Grades: semantic (LSP — proven identity, project-scoped) / structural (AST — proven shape) / lexical (grep — total coverage, proves nothing about identity) / provider (GitHub index — weakest).
- Never conclude impact or absence from one grade.
- Before any "unused / only-used-in-Y" claim: diff one package-wide grep (including tests/scripts/configs) against the LSP result — every lexical hit LSP missed is a finding (re-export, shadow copy, string/config reference).
- Empty ≠ absence: `status:"empty"` proves only the searched scope; GitHub code search is default-branch-only and blind to archived/renamed repos — verify the path exists or materialize+grep before claiming absence.
- Read completeness metadata (`truncatedByDepth`, `dynamicCallsExcluded`, skipped counts) before claiming full impact; LSP "unsupported" = capability absence, not "no usage".

**Economy:** batch up to 5 independent queries per tool call; follow returned `next.*`/pagination cursors verbatim (never compute offsets); tighten scope → leaner mode (concise/symbols/counts) → smaller pages → only then paginate.
