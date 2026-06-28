#!/usr/bin/env node
// Custom-LSP live check — proves the BRING-YOUR-OWN-SERVER path end-to-end with
// a REAL server, for a language octocode has NO built-in support for.
//
// Bash/shell was removed from built-in LSP routing, so `bash-language-server`
// can ONLY work through a user `.octocode/lsp-servers.json`. This check:
//   1. confirms `.sh` resolves to NO server by default (no-fallback baseline),
//   2. registers bash-language-server via a temp `.octocode/lsp-servers.json`,
//   3. spawns the real server through the engine's NativeLspClient and asserts
//      real semantic results (documentSymbols + references + hover).
//
// Environment-dependent: if bash-language-server isn't installed it SKIPS and
// exits 0 (so it is NOT part of run-all). Install + run:
//   npm i -g bash-language-server   (or set OCTOCODE_BASH_LS_BIN=<abs path>)
//   yarn lsp:custom

import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { engine } from '../_engine.mjs'

const skip = (msg) => { console.log(`↷ custom-LSP check SKIPPED: ${msg}`); process.exit(0) }

// Resolve a bash-language-server the engine can launch: explicit override, then
// the bare command on PATH. (Custom configs accept a bare name or absolute path.)
const candidates = [process.env.OCTOCODE_BASH_LS_BIN, 'bash-language-server'].filter(Boolean)
const serverBin = candidates.find((c) => { try { return engine.isCommandAvailable(c) } catch { return false } })
if (!serverBin) skip('bash-language-server not found (npm i -g bash-language-server, or set OCTOCODE_BASH_LS_BIN)')

const SAMPLE = `#!/usr/bin/env bash
set -euo pipefail

greeting="hello"

greet() {
  local name="$1"
  echo "$greeting, $name"
}

main() {
  greet "world"
  greet "octocode"
}

main "$@"
`

const ws = mkdtempSync(join(tmpdir(), 'octo-custom-lsp-'))
const file = join(ws, 'deploy.sh')
writeFileSync(file, SAMPLE)

const failures = []
const rec = (op, ok, detail) => { if (!ok) failures.push(op); console.log(`${ok ? '✓' : '✗'} ${op.padEnd(20)} ${detail}`) }

console.log(`Custom-LSP end-to-end — unsupported language (bash) via .octocode/lsp-servers.json\n`)

// 1) No built-in server for .sh (the no-fallback baseline).
const baseline = engine.getLanguageServerForFile(file, ws)
rec('no-config baseline', baseline === null, baseline === null ? 'no server → semantic ops throw lspServerUnavailable' : `UNEXPECTED ${JSON.stringify(baseline)}`)

// 2) Register the server via a project-local config.
mkdirSync(join(ws, '.octocode'), { recursive: true })
writeFileSync(join(ws, '.octocode/lsp-servers.json'), JSON.stringify({
  languageServers: { '.sh': { command: serverBin, args: ['start'], languageId: 'shellscript' } },
}, null, 2))
const cfg = engine.getLanguageServerForFile(file, ws)
rec('config resolves', !!cfg && !!cfg.command, cfg ? `${cfg.command} ${JSON.stringify(cfg.args)} (${cfg.languageId})` : 'NOT resolved')

// 3) Spawn the real server and exercise it.
if (cfg && cfg.command) {
  const client = new engine.NativeLspClient(cfg)
  try {
    await client.start()
    try { await client.waitForReady(8000) } catch { /* bash-ls has no $/progress */ }
    await client.openDocument(file, SAMPLE)
    await new Promise((r) => setTimeout(r, 1500))

    const sym = await client.getDocumentSymbols(file)
    const names = (Array.isArray(sym) ? sym : []).map((s) => s.name || s)
    rec('documentSymbols', names.length > 0, `${names.length} symbols ${JSON.stringify(names.slice(0, 6))}`)

    const refs = await client.getReferences(file, 5, 0, true) // `greet` definition (line 6)
    rec('references', Array.isArray(refs) && refs.length >= 2, `${(refs || []).length} reference(s) of greet`)

    const hov = await client.getHover(file, 7, 2) // `echo` builtin (line 8)
    rec('hover', !!hov && JSON.stringify(hov).length > 2, `${hov ? JSON.stringify(hov).length : 0} chars of docs`)

    await client.stop()
  } catch (e) {
    console.error('server stderr:', (client.getRecentStderr?.() || []).slice(-8).join('\n'))
    failures.push(`spawn/ops: ${e.message}`)
  }
}

rmSync(ws, { recursive: true, force: true })

console.log()
if (failures.length) { console.error(`✗ custom-LSP check FAILED: ${failures.join(', ')}`); process.exit(1) }
console.log(`✓ custom LSP works end-to-end: ${serverBin.split('/').pop()} registered via\n  .octocode/lsp-servers.json produced REAL semantics for an unsupported language.`)
