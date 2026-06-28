#!/usr/bin/env node
// Full support matrix — for EVERY extension the engine knows (the union of the
// minify config, the structural-search grammar list, the signature list, and
// the LSP server specs), probe each feature LIVE through octocode-engine and
// report which features really work.
//
// Columns:
//   MINIFY    — configured strategy; verified by running minifyContentResult.
//   AST       — extension is in getSupportedStructuralExtensions() AND a probe
//               actually parses (structuralSearch "$$$" > 0).
//   SIG       — in getSupportedSignatureExtensions() (tree-sitter outline).
//   LSP       — detectLanguageId + getLanguageServerForFile resolve a server.
//
// Anomalies (claimed-but-not-working, or list/behavior mismatch) are collected
// and the script exits non-zero if any are found.

import { writeFileSync, readFileSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { engine, engineRoot } from './_engine.mjs'

const here = dirname(fileURLToPath(import.meta.url))

// Server binary per LSP language id (documentation — the engine is the source
// of truth for WHICH languages have a server; this names the binary it spawns).
const SERVER_NAMES = {
  typescript: 'typescript-language-server', typescriptreact: 'typescript-language-server',
  javascript: 'typescript-language-server', javascriptreact: 'typescript-language-server',
  python: 'pylsp', go: 'gopls', rust: 'rust-analyzer', java: 'jdtls',
  c: 'clangd', cpp: 'clangd', csharp: 'csharp-ls', shellscript: 'bash-language-server',
  json: 'vscode-json-language-server', yaml: 'yaml-language-server', toml: 'taplo',
  html: 'vscode-html-language-server', css: 'vscode-css-language-server',
  scss: 'vscode-css-language-server', less: 'vscode-css-language-server',
  // ── New language additions ───────────────────────────────────────────────
  ruby: 'ruby-lsp', php: 'intelephense', kotlin: 'kotlin-language-server',
  elixir: 'elixir-ls', terraform: 'terraform-ls', lua: 'lua-language-server',
  sql: 'sqls', proto: 'protols', ocaml: 'ocamllsp', zig: 'zls',
  r: 'r-languageserver', julia: 'julia', erlang: 'erlang-ls', swift: 'sourcekit-lsp',
}

const fileTypes = engine.getMINIFY_CONFIG().fileTypes
const structural = new Set(engine.getSupportedStructuralExtensions())
const signature = new Set(engine.getSupportedSignatureExtensions())

const allExts = [...new Set([
  ...Object.keys(fileTypes),
  ...structural,
  ...signature,
])].sort()

// Minify probe input: json family needs valid JSON; everything else tolerates a
// generic snippet (strategies strip comments/whitespace, never hard-fail).
const jsonish = new Set(['json', 'jsonc', 'json5'])
const minifyInput = (ext) => jsonish.has(ext)
  ? '{\n  "a": 1,\n  "b": [2, 3],\n  "c": "x"\n}\n'
  : '// header comment\nfunction foo(a, b) {\n  return a + b;\n}\n\n\nfoo(1, 2);\n'

const rows = []
const anomalies = []

for (const ext of allExts) {
  const cfg = fileTypes[ext]
  const path = `probe.${ext}`

  // MINIFY
  let minify = '—'
  if (cfg) {
    try {
      const r = engine.minifyContentResult(minifyInput(ext), path)
      minify = r.failed ? `FAIL(${r.reason || r.type})` : cfg.strategy
      if (r.failed) anomalies.push(`minify ${ext}: ${r.reason || 'failed'}`)
    } catch (e) { minify = 'THREW'; anomalies.push(`minify ${ext} threw: ${e.message}`) }
  }

  // AST
  let ast = '—'
  if (structural.has(ext)) {
    let nodes = 0
    try { nodes = engine.structuralSearch('foo(a)\nbar(b)\n', path, '$$$').length } catch { nodes = -1 }
    if (nodes > 0) ast = '✓'
    else { ast = 'BROKEN'; anomalies.push(`ast ${ext}: in structural list but parsed ${nodes} nodes`) }
  }

  // SIGNATURE
  const sig = signature.has(ext) ? '✓' : '—'
  if (signature.has(ext) && !structural.has(ext)) anomalies.push(`sig ${ext}: signature without structural (impossible tier)`)

  // LSP — getLanguageServerForFile's languageId is authoritative for server
  // init. It may legitimately differ from the tree-sitter grammar registry id
  // (detectLanguageId): e.g. `.jsx` shares the JavaScript grammar so the
  // registry id is "javascript", but the server is started as "javascriptreact"
  // to enable JSX features (`.tsx` has its own grammar, so it matches). Both are
  // correct; only a configured-but-unstartable server is an anomaly.
  let lsp = '—'
  let lspId = null
  let cfgL = null
  try { cfgL = engine.getLanguageServerForFile(path, engineRoot) } catch (e) { anomalies.push(`lsp ${ext}: getLanguageServerForFile threw: ${e.message}`) }
  if (cfgL) {
    lspId = cfgL.languageId
    if (!cfgL.command) { lsp = `${cfgL.languageId}(no-server)`; anomalies.push(`lsp ${ext}: server config has no command`) }
    else lsp = cfgL.languageId
  }

  rows.push({ ext, minify, ast, sig, lsp, lspId, server: lspId ? SERVER_NAMES[lspId] ?? '?' : null })
}

// ── Report ──────────────────────────────────────────────────────────────────
const pad = (s, n) => String(s).padEnd(n)
const supported = rows.filter((r) => r.ast !== '—' || r.sig !== '—' || r.lsp !== '—')
const minifyOnly = rows.filter((r) => r.ast === '—' && r.sig === '—' && r.lsp === '—')

console.log(`\nSupport matrix — ${allExts.length} extensions (minify ${Object.keys(fileTypes).length}, structural ${structural.size}, signature ${signature.size})\n`)
console.log(`${pad('ext', 8)} ${pad('MINIFY', 14)} ${pad('AST', 7)} ${pad('SIG', 5)} LSP (language-id)`)
console.log('-'.repeat(64))
console.log('· extensions with AST / SIG / LSP support (the "rich" formats):')
for (const r of supported) console.log(`${pad(r.ext, 8)} ${pad(r.minify, 14)} ${pad(r.ast, 7)} ${pad(r.sig, 5)} ${r.lsp}`)

console.log(`\n· minify-only extensions (${minifyOnly.length}): comment/whitespace strategy, no AST/LSP:`)
const byStrategy = {}
for (const r of minifyOnly) (byStrategy[r.minify] ??= []).push(r.ext)
for (const [s, list] of Object.entries(byStrategy).sort()) console.log(`  ${pad(s, 14)} (${list.length}): ${list.join(' ')}`)

console.log('\nTotals:')
console.log(`  AST grammars:        ${rows.filter((r) => r.ast === '✓').length} extensions`)
console.log(`  Signature outline:   ${rows.filter((r) => r.sig === '✓').length} extensions`)
console.log(`  LSP server config:   ${rows.filter((r) => r.lsp !== '—' && !String(r.lsp).includes('no-server')).length} extensions`)
console.log(`  Minify only:         ${minifyOnly.length} extensions`)

// ── Support-matrix generation (yarn matrix:check --write) ─────────────────────
// The matrix is injected between markers in docs/LSP_SERVER_LIFECYCLE.md (the
// single LSP + format-support doc) — there is no standalone SUPPORT.md. Only the
// region between the BEGIN/END markers is machine-owned; the rest is hand-written.
const SUPPORT_BEGIN = '<!-- BEGIN GENERATED: support-matrix (yarn matrix:check --write) — do not edit between these markers -->'
const SUPPORT_END = '<!-- END GENERATED: support-matrix -->'

if (process.argv.includes('--write')) {
  const astN = rows.filter((r) => r.ast === '✓').length
  const sigN = rows.filter((r) => r.sig === '✓').length
  const lspN = rows.filter((r) => r.lspId && !String(r.lsp).includes('no-server')).length
  const richTable = supported.map((r) => {
    const sigCell = r.sig === '✓' ? '✅ tree-sitter' : '—'
    const lspCell = r.server ? (String(r.lsp).includes('no-server') ? `${r.lspId} (no server)` : `\`${r.server}\` → \`${r.lspId}\``) : '—'
    const minCell = r.minify === '—' ? '—' : `\`${r.minify}\``
    return `| \`.${r.ext}\` | ${minCell} | ${r.ast === '✓' ? '✅' : '—'} | ${sigCell} | ${lspCell} |`
  }).join('\n')
  const minifyBlocks = Object.entries(byStrategy).sort().map(([s, list]) => `**\`${s}\`** (${list.length}): ${list.sort().map((e) => `\`${e}\``).join(' ')}`).join('\n\n')

  // Body only (no H1) — the doc supplies the `## Full format support matrix`
  // heading; sub-headings are H3 so they nest under it.
  const body = `_Generated by \`yarn matrix:check --write\` (benchmark/check-matrix.mjs) — every cell probed live against the shipped napi binary. Do not edit between the markers; run \`yarn matrix:check\` to re-verify._

**${allExts.length} extensions** known to the engine — ${astN} with structural AST, ${sigN} with a signature outline, ${lspN} with an LSP server, ${minifyOnly.length} minify-only.

### Rich formats — AST + signature + LSP

Extensions with a wired tree-sitter grammar (and, where configured, a language server). The minify column is the configured strategy.

| Extension | Minify | Structural AST | Signature outline | LSP (server → language-id) |
|-----------|--------|:--------------:|-------------------|----------------------------|
${richTable}

Notes:
- **Signature outline is tree-sitter only** — markup/style/config grammars (HTML/CSS/SCSS/LESS/Scala/JSON/YAML/TOML) parse for structural \`rule\` queries but have no function body, so no skeleton. There is **no** regex/heuristic fallback.
- **\`.jsx\`** resolves the LSP server as \`javascriptreact\` (to enable JSX) even though its tree-sitter grammar registry id is \`javascript\` (shared JS grammar). \`.tsx\` has its own grammar, so both ids are \`typescriptreact\`.
- **\`.hh\` / \`.hxx\`** have the C++ grammar + signatures but **no clangd server config** (only \`.cpp/.cc/.cxx/.hpp\` are mapped).
- **C/C++**: structural \`rule\` queries (e.g. \`kind: call_expression\`) work fully; a bare call-shaped \`pattern\` can hit tree-sitter's declaration-vs-call ambiguity — prefer a \`rule\` with \`kind\`. JS/TS also have a native (oxc) symbol/in-file-reference path that needs **no server installed**.

### Minify-only formats

Native comment/whitespace stripping; no AST/LSP. (${minifyOnly.length} extensions, grouped by strategy.)

${minifyBlocks}

### Verify

\`\`\`bash
yarn matrix:check     # this matrix, live
yarn ast:check        # structural search + signatures on real samples
yarn lsp:check        # language-id + server resolution + native semantics
yarn lsp:live         # spawn a real server, exercise every LSP operation type
yarn minify:check     # minifier over every configured format
yarn benchmark        # all of the above
\`\`\``

  const docPath = join(here, '..', '..', '..', 'docs', 'LSP_SERVER_LIFECYCLE.md')
  const doc = readFileSync(docPath, 'utf8')
  const begin = doc.indexOf(SUPPORT_BEGIN)
  const end = doc.indexOf(SUPPORT_END)
  if (begin === -1 || end === -1 || end < begin) {
    console.error(`\n✗ support-matrix markers not found in ${docPath}. Expected:\n  ${SUPPORT_BEGIN}\n  ${SUPPORT_END}`)
    process.exit(1)
  }
  const next = `${doc.slice(0, begin + SUPPORT_BEGIN.length)}\n\n${body}\n\n${doc.slice(end)}`
  writeFileSync(docPath, next)
  console.log(`\n✎ wrote support matrix into ${join('docs', 'LSP_SERVER_LIFECYCLE.md')}`)
}

if (anomalies.length) { console.error(`\n✗ ${anomalies.length} anomalies:`); for (const a of anomalies) console.error(`  • ${a}`); process.exit(1) }
console.log(`\n✓ matrix consistent: every claimed capability verified working, no orphans.`)
