# LSP benchmark

Proves the `octocode-engine` LSP layer is wired correctly for every language
with a configured language server (`src/lsp/config.rs`) — run over the **real**
files in `samples/` (provenance in `manifest.json`).

```bash
node benchmark/lsp/check-lsp.mjs      # or: yarn lsp:check
```

Verified **without** needing the external servers installed:

1. **LANGUAGE-ID** — `detectLanguageId(<sample>)` equals the expected LSP
   language id.
2. **SERVER** — `getLanguageServerForFile(<sample>)` resolves a non-empty
   command and the expected `languageId` (spec → invocation resolution).
3. **NATIVE SEMANTICS** — in-process, no server required:
   - boundary-capable languages: `getSemanticBoundaryOffsets > 0` (tree-sitter).
   - JS/TS family: `extractJsSymbols` returns symbols (oxc native).

**Server availability** is reported (`isCommandAvailable`) but never fails the
check — the external servers are optional and installed per developer.

Beyond the per-language table, the check also proves three contracts:

4. **CUSTOM LSP** — Scala has no built-in server, but a temp
   `.octocode/lsp-servers.json` registering `metals` makes
   `getLanguageServerForFile` resolve it (the bring-your-own path).
5. **NO-FALLBACK** — the same Scala file *without* a config resolves to no
   server (downstream the engine throws `lspServerUnavailable` → the agent
   falls back to text/structural search).
6. **MARKUP → MINIFY** — Markdown/MDX are **not** LSP: they resolve to no
   server and instead route to the minifier's heading-section heuristics
   (`## ` headings preserved). HTML/CSS *are* LSP (asserted in the table).

Real server-backed navigation (cross-file references, go-to-definition) is
exercised live by `check-lsp-live.mjs` (`yarn lsp:live`, TypeScript), not this
wiring check.

## Custom-LSP live check (`check-custom-lsp.mjs`)

Proves the **bring-your-own-server** path end-to-end with a *real* server, for a
language octocode has **no built-in support for**. Bash/shell was removed from
built-in routing, so `bash-language-server` can only work through a user
`.octocode/lsp-servers.json` — which makes it the perfect proof:

```bash
npm i -g bash-language-server   # or set OCTOCODE_BASH_LS_BIN=<abs path>
yarn lsp:custom
```

It (1) confirms `.sh` resolves to **no** server by default (the no-fallback
baseline), (2) registers `bash-language-server` via a temp config, then
(3) spawns the real server through `NativeLspClient` and asserts real
`documentSymbols` + `references` + `hover` results. Environment-dependent: it
**skips and exits 0** if the server isn't installed (so it is not in run-all).
