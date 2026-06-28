#!/usr/bin/env node
// LSP benchmark check — proves the octocode-engine LSP layer is wired correctly
// for every language that has a configured language server (src/lsp/config.rs),
// run over the REAL files in lsp/samples/ (see lsp/manifest.json for provenance).
//
// What it verifies WITHOUT needing the external servers installed:
//   LANGUAGE-ID — detectLanguageId(<sample>) equals the expected LSP language id.
//   SERVER      — getLanguageServerForFile(<sample>) resolves a non-empty command
//                 and the expected languageId (the spec → invocation resolution).
//   SEMANTICS   — native, in-process semantics that need no server:
//                   • boundary-capable langs: getSemanticBoundaryOffsets > 0
//                     (tree-sitter boundaries — the signature/chunk path).
//                   • JS/TS family: extractJsSymbols returns symbols (oxc native).
//
// SERVER AVAILABILITY is reported (isCommandAvailable) but never fails the check —
// the servers are optional, installed per-developer.
//
// It also proves three contracts beyond the per-language wiring table:
//   CUSTOM LSP  — an unsupported language (Scala) has NO built-in server, but a
//                 user `.octocode/lsp-servers.json` registers one (metals) and it
//                 resolves. This is the bring-your-own-server path (with/without).
//   NO-FALLBACK — Scala without a config resolves to nothing (the engine throws
//                 lspServerUnavailable downstream → agent falls back to search).
//   MARKUP/MINIFY — Markdown/MDX are NOT LSP; they route to the minifier's
//                 heading-section heuristics. HTML/CSS ARE LSP (asserted above).
//
// Exits non-zero on any wiring failure.

import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { engine, engineRoot, loadManifestSamples, pad } from '../_engine.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const samples = loadManifestSamples(here)

// ext -> expected LSP language id + native-semantics capabilities.
// `boundary`: getSemanticBoundaryOffsets is backed by tree-sitter (sig grammars).
// `jsSymbols`: extractJsSymbols (oxc) applies (JS/TS family only).
const LANGS = [
  { name: 'TypeScript', ext: 'ts', id: 'typescript', boundary: true, jsSymbols: true },
  { name: 'TSX', ext: 'tsx', id: 'typescriptreact', boundary: true, jsSymbols: true },
  { name: 'JavaScript', ext: 'js', id: 'javascript', boundary: true, jsSymbols: true },
  { name: 'Python', ext: 'py', id: 'python', boundary: true, jsSymbols: false },
  { name: 'Go', ext: 'go', id: 'go', boundary: true, jsSymbols: false },
  { name: 'Rust', ext: 'rs', id: 'rust', boundary: true, jsSymbols: false },
  { name: 'Java', ext: 'java', id: 'java', boundary: true, jsSymbols: false },
  { name: 'C', ext: 'c', id: 'c', boundary: true, jsSymbols: false },
  { name: 'C++', ext: 'cpp', id: 'cpp', boundary: true, jsSymbols: false },
  { name: 'C#', ext: 'cs', id: 'csharp', boundary: true, jsSymbols: false },
  { name: 'Swift', ext: 'swift', id: 'swift', boundary: true, jsSymbols: false },
  { name: 'SQL', ext: 'sql', id: 'sql', boundary: false, jsSymbols: false },
  { name: 'JSON', ext: 'json', id: 'json', boundary: false, jsSymbols: false },
  { name: 'YAML', ext: 'yaml', id: 'yaml', boundary: false, jsSymbols: false },
  { name: 'HTML', ext: 'html', id: 'html', boundary: false, jsSymbols: false },
  { name: 'CSS', ext: 'css', id: 'css', boundary: false, jsSymbols: false },
  { name: 'SCSS', ext: 'scss', id: 'scss', boundary: false, jsSymbols: false },
  { name: 'Less', ext: 'less', id: 'less', boundary: false, jsSymbols: false },
]

const rows = []
const failures = []

for (const l of LANGS) {
  const issues = []
  const { content, path } = samples.readSample(l.ext, issues)
  const probePath = path || `probe.${l.ext}`

  // LANGUAGE-ID
  let id = null
  try { id = engine.detectLanguageId(probePath) } catch (e) { issues.push(`detectLanguageId threw: ${e.message}`) }
  if (id !== l.id) issues.push(`languageId got ${JSON.stringify(id)}, expected ${l.id}`)

  // SERVER resolution
  let cfg = null
  try { cfg = engine.getLanguageServerForFile(probePath, engineRoot) } catch (e) { issues.push(`getLanguageServerForFile threw: ${e.message}`) }
  if (!cfg || !cfg.command) issues.push('no server config resolved')
  else if (cfg.languageId !== l.id) issues.push(`server languageId ${JSON.stringify(cfg.languageId)} != ${l.id}`)

  // SERVER availability — informational only
  let avail = '—'
  if (cfg && cfg.command) { try { avail = engine.isCommandAvailable(cfg.command) ? 'installed' : 'absent' } catch { avail = '?' } }

  // NATIVE SEMANTICS
  // Default 'server-backed' = no native probe for this language (semantics come
  // from the external server). When the sample is missing/corrupt, readSample
  // already failed the row — label it 'skipped' so the table doesn't imply the
  // native probe ran.
  let sem = content ? 'server-backed' : 'skipped (no sample)'
  if (content) {
    if (l.boundary) {
      let n = 0
      try { n = engine.getSemanticBoundaryOffsets(content, probePath).length } catch (e) { issues.push(`boundaries threw: ${e.message}`) }
      if (n <= 0) issues.push('getSemanticBoundaryOffsets returned 0')
      sem = `${n} boundaries`
    }
    if (l.jsSymbols) {
      let syms = null
      try { syms = engine.extractJsSymbols(content, probePath) } catch (e) { issues.push(`extractJsSymbols threw: ${e.message}`) }
      let count = 0
      if (syms) { try { count = JSON.parse(syms).length } catch { /* non-JSON */ } }
      if (!syms || count <= 0) issues.push('extractJsSymbols returned no symbols')
      sem += `, ${count} symbols`
    }
  }

  const ok = issues.length === 0
  if (!ok) failures.push(`${l.name}: ${issues.join('; ')}`)
  rows.push({ name: l.name, ext: l.ext, id: l.id, avail, sem, ok })
}

console.log(`\nLSP benchmark — ${rows.length} languages over real samples (lsp/samples/)`)
console.log(`${pad('Language', 12)} ${pad('ext', 6)} ${pad('languageId', 16)} ${pad('server', 10)} ${pad('native semantics', 24)} status`)
console.log('-'.repeat(82))
for (const r of rows) console.log(`${pad(r.name, 12)} ${pad(r.ext, 6)} ${pad(r.id, 16)} ${pad(r.avail, 10)} ${pad(r.sem, 24)} ${r.ok ? 'PASS' : 'FAIL'}`)
console.log(`\nServer column is informational (external servers are optional).`)

// ── CUSTOM LSP (bring-your-own) + NO-FALLBACK contract ───────────────────────
// Scala has no built-in server. Prove: (1) it resolves to nothing by default
// (→ engine throws lspServerUnavailable → agent falls back to search), and
// (2) a user `.octocode/lsp-servers.json` registers metals and it resolves.
console.log(`\nCustom LSP — bring-your-own server (Scala / metals)`)
{
  const issues = []
  const ws = mkdtempSync(join(tmpdir(), 'octo-lsp-custom-'))
  try {
    const scala = join(ws, 'List.scala')
    writeFileSync(scala, 'object Demo { def main(args: Array[String]): Unit = () }\n')

    // (1) WITHOUT config — no built-in server (the no-fallback baseline).
    let before = null
    try { before = engine.getLanguageServerForFile(scala, ws) } catch (e) { issues.push(`without-config threw: ${e.message}`) }
    if (before) issues.push(`expected NO built-in server for .scala, got ${JSON.stringify(before.command)}`)
    console.log(`  ${pad('without config', 22)} ${before ? 'resolved ' + before.command : 'no server (→ search fallback)'}`)

    // (2) WITH config — user registers metals for .scala.
    mkdirSync(join(ws, '.octocode'), { recursive: true })
    writeFileSync(join(ws, '.octocode/lsp-servers.json'), JSON.stringify({
      languageServers: { '.scala': { command: 'metals', args: ['stdio'], languageId: 'scala' } },
    }))
    let after = null
    try { after = engine.getLanguageServerForFile(scala, ws) } catch (e) { issues.push(`with-config threw: ${e.message}`) }
    if (!after || after.command !== 'metals') issues.push(`custom server not resolved (got ${JSON.stringify(after)})`)
    else if (after.languageId !== 'scala') issues.push(`custom languageId ${JSON.stringify(after.languageId)} != scala`)
    console.log(`  ${pad('with config', 22)} ${after ? `resolved ${after.command} (${after.languageId})` : 'NOT resolved'}`)
  } finally {
    rmSync(ws, { recursive: true, force: true })
  }
  if (issues.length) failures.push(`Custom LSP (Scala): ${issues.join('; ')}`)
}

// ── MARKUP vs MINIFY — Markdown/MDX are NOT LSP; they use minify heuristics ───
// HTML/CSS are asserted as LSP in the table above. Markdown/MDX must resolve to
// NO server and instead compress via the markdown strategy (heading sections).
console.log(`\nMarkup routing — Markdown/MDX → minify (not LSP)`)
for (const ext of ['md', 'mdx']) {
  const issues = []
  const probe = `doc.${ext}`
  let cfg = 'present'
  try { cfg = engine.getLanguageServerForFile(probe, engineRoot) } catch (e) { issues.push(`getLanguageServerForFile threw: ${e.message}`) }
  if (cfg) issues.push(`expected NO LSP server for .${ext}, got ${JSON.stringify(cfg.command)}`)

  // The minifier keeps section structure (## headings) — its core heuristic.
  const src = '# Title\n\n## Section\n\nSome ![badge](https://img.shields.io/x) text with emoji 🚀.\n'
  let res = null
  try { res = engine.minifyContentResult(src, probe) } catch (e) { issues.push(`minify threw: ${e.message}`) }
  const minified = res && res.content
  if (!minified || res.failed) issues.push('minify failed/empty')
  else if (!minified.includes('## Section')) issues.push('minify dropped the ## heading (section heuristic broken)')
  console.log(`  ${pad('.' + ext, 22)} ${cfg ? 'LSP?! ' + cfg.command : 'no server'} · minify keeps headings: ${minified && minified.includes('## Section') ? 'yes' : 'NO'}`)
  if (issues.length) failures.push(`Markup .${ext}: ${issues.join('; ')}`)
}

if (failures.length) { console.error(`\n✗ LSP check FAILED (${failures.length}):`); for (const f of failures) console.error(`  • ${f}`); process.exit(1) }
console.log(`\n✓ all ${rows.length} LSP languages wired correctly + custom-LSP (Scala) + markup→minify routing verified.`)
