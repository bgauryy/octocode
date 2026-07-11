#!/usr/bin/env node
// Minify benchmark check — runs the octocode-engine minifier over every sample
// in this folder (one per discovered extension) and asserts it produces output
// without failing. This is the "is minify working for every configured format"
// gate; the size/strategy report is regenerated separately by
// generate-real-code-report.mjs.
//
// Each minify/<lang>/ holds raw/source.excerpt.txt (a real excerpt) and
// raw/metadata.json (provenance, incl. the original source filename → its
// extension). The minifier picks its strategy from that extension.
//
// Exits non-zero if any sample fails to minify (failed=true or empty output).
// Real fixtures prove language quality. Language-specific synthetic fixtures
// prove dispatch for extensions without real samples. Generic synthetic fixtures
// are allowed only by explicit allowlist so new extensions cannot hide here.

import { readFileSync, existsSync, readdirSync } from 'node:fs'
import { join, dirname, extname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { engine } from '../_engine.mjs'

const here = dirname(fileURLToPath(import.meta.url))
const fileTypes = engine.getMINIFY_CONFIG().fileTypes

// Discover sample dirs: any subdir with raw/source.excerpt.txt.
const dirs = readdirSync(here, { withFileTypes: true })
  .filter((d) => d.isDirectory() && existsSync(join(here, d.name, 'raw', 'source.excerpt.txt')))
  .map((d) => d.name)
  .sort()

const rows = []
const failures = []
const cases = []
const realExtensions = new Set()
const GENERIC_SYNTHETIC_ALLOWLIST = new Set([
  'awk',
  'bzl',
  'cmake',
  'coffee',
  'csv',
  'elm',
  'ex',
  'exs',
  'haml',
  'jade',
  'log',
  'nix',
  'pas',
  'pp',
  'pug',
  'slim',
  'star',
  'txt',
  'v',
  'zig',
])

for (const dir of dirs) {
  const base = join(here, dir, 'raw')
  const content = readFileSync(join(base, 'source.excerpt.txt'), 'utf8')
  // Prefer the real source filename's extension from metadata; fall back to dir.
  let ext = dir
  const metaPath = join(base, 'metadata.json')
  if (existsSync(metaPath)) {
    try { const m = JSON.parse(readFileSync(metaPath, 'utf8')); if (m.source) ext = extname(m.source).replace(/^\./, '') || dir } catch (e) { throw new Error(`malformed ${metaPath}: ${e.message}`) }
  }
  ext = ext.toLowerCase()
  realExtensions.add(ext)
  cases.push({ name: dir, ext, content, kind: 'real' })
}

for (const ext of Object.keys(fileTypes).sort()) {
  if (realExtensions.has(ext)) continue
  const synthetic = syntheticSampleFor(ext)
  cases.push({
    name: `synthetic:${ext}`,
    ext,
    content: synthetic.content,
    kind: synthetic.kind,
  })
}

for (const testCase of cases) {
  const { name, ext, content, kind } = testCase
  const filePath = `sample.${ext}`

  if (kind === 'genericSynthetic' && !GENERIC_SYNTHETIC_ALLOWLIST.has(ext)) {
    failures.push(`${name}: generic synthetic fixture is not allowlisted; add a real or language-specific sample`)
  }

  let res = null
  try { res = engine.minifyContentResult(content, filePath) } catch (e) {
    failures.push(`${name}: minify threw: ${e.message}`)
    // Keep the row visible in the table (as ERROR) instead of silently dropping
    // it — a missing row mid-table is easy to overlook.
    rows.push({ name, ext, kind, type: 'ERROR', inB: content.length, outB: 0, cut: '—', ok: false })
    continue
  }

  const out = res.content || ''
  const ok = !res.failed && out.length > 0
  if (!ok) failures.push(`${name}: failed=${res.failed}${res.reason ? ` (${res.reason})` : ''}, out=${out.length}B`)

  const deterministic = JSON.stringify({
    failed: res.failed,
    type: res.type,
    reason: res.reason ?? null,
    content: res.content,
  })
  for (let i = 0; i < 10; i += 1) {
    const next = engine.minifyContentResult(content, filePath)
    const snapshot = JSON.stringify({
      failed: next.failed,
      type: next.type,
      reason: next.reason ?? null,
      content: next.content,
    })
    if (snapshot !== deterministic) {
      failures.push(`${name}: minifyContentResult is not deterministic on repeat ${i + 1}`)
      break
    }
  }

  for (const [api, fn] of [
    ['applyMinification', engine.applyMinification],
    ['applyContentViewMinification', engine.applyContentViewMinification],
  ]) {
    const first = fn(content, filePath)
    if (first.length > content.length) {
      failures.push(`${name}: ${api} grew output from ${content.length}B to ${first.length}B`)
    }
    for (let i = 0; i < 10; i += 1) {
      const next = fn(content, filePath)
      if (next !== first) {
        failures.push(`${name}: ${api} is not deterministic on repeat ${i + 1}`)
        break
      }
    }
  }

  const cut = content.length > 0 ? (((content.length - out.length) / content.length) * 100).toFixed(1) : '0.0'
  rows.push({ name, ext, kind, type: res.type, inB: content.length, outB: out.length, cut, ok })
}

const testedExtensions = new Set(cases.map((testCase) => testCase.ext))
for (const ext of Object.keys(fileTypes).sort()) {
  if (!testedExtensions.has(ext)) failures.push(`configured extension .${ext} was not tested`)
}

// ── Adversarial regression: literal preservation + re-parseability ─────────
// The checks above only assert "didn't fail, produced non-empty output,
// didn't grow, and is deterministic" — none of which catch a minifier that
// corrupts string/regex/template literal content or JSON numeric precision
// while still producing plausible-looking, deterministic output. These cases
// pin the exact adversarial inputs found by a prior audit so a regression
// fails loudly here instead of shipping silently.
const adversarialCases = [
  {
    name: 'adversarial:lua-string-content',
    ext: 'lua',
    content: 'local s = "hello    world"\nlocal t = "a: b, c"',
    check: (out) => out.includes('hello    world') && out.includes('a: b, c'),
    describe: 'aggressive strategy must not collapse whitespace or tighten punctuation inside a string literal',
  },
  {
    name: 'adversarial:elixir-newline-separator',
    ext: 'exs',
    content: 'x = 1\ny = 2',
    check: (out) => out.includes('\n'),
    describe: 'aggressive strategy must not flatten a newline statement separator into a space',
  },
  {
    name: 'adversarial:js-template-literal',
    ext: 'mjs',
    content: 'const s = `key: ${v}, end`; @@@ deliberately invalid syntax so OXC declines and the heuristic fallback runs',
    check: (out) => out.includes('`key: ${v}, end`'),
    describe: 'JS heuristic fallback must not mutate template literal content',
  },
  {
    name: 'adversarial:json-bignum-precision',
    ext: 'json',
    content: '{"n": 123456789012345678901234567890, "pi": 3.14159265365358979}',
    check: (out) => {
      let parsed
      try { parsed = JSON.parse(out) } catch { return false }
      return out.includes('123456789012345678901234567890') && out.includes('3.14159265365358979') && parsed !== null
    },
    describe: 'JSON minify must not lose numeric precision on large integers or long decimals',
  },
  {
    name: 'adversarial:json-key-order',
    ext: 'json',
    content: '{"z": 1, "a": 2, "m": 3}',
    check: (out) => out.indexOf('"z"') < out.indexOf('"a"') && out.indexOf('"a"') < out.indexOf('"m"'),
    describe: 'JSON minify must preserve original key order',
  },
]

for (const { name, ext, content, check, describe } of adversarialCases) {
  const filePath = `sample.${ext}`
  let res = null
  try { res = engine.minifyContentResult(content, filePath) } catch (e) {
    failures.push(`${name}: minify threw: ${e.message}`)
    continue
  }
  const out = res.content || ''
  if (res.failed) {
    failures.push(`${name}: minify reported failed=true, out=${out.length}B (${describe})`)
    continue
  }
  if (!check(out)) {
    failures.push(`${name}: literal/precision was corrupted — ${describe}\n    in:  ${JSON.stringify(content)}\n    out: ${JSON.stringify(out)}`)
  }
}
console.log(`\nAdversarial regression: ${adversarialCases.length} cases (literal preservation + JSON precision/order)`)

const pad = (s, n) => String(s).padEnd(n)
const padL = (s, n) => String(s).padStart(n)
const realCount = rows.filter((row) => row.kind === 'real').length
const languageSyntheticCount = rows.filter((row) => row.kind === 'languageSynthetic').length
const genericSyntheticCount = rows.filter((row) => row.kind === 'genericSynthetic').length
console.log(`\nMinify benchmark — ${rows.length} samples (${realCount} real, ${languageSyntheticCount} language synthetic, ${genericSyntheticCount} generic synthetic)`)
console.log(`${pad('sample', 20)} ${pad('kind', 10)} ${pad('ext', 8)} ${pad('strategy', 14)} ${padL('in', 8)} ${padL('out', 8)} ${padL('cut%', 7)} status`)
console.log('-'.repeat(88))
for (const r of rows) console.log(`${pad(r.name, 20)} ${pad(r.kind, 10)} ${pad(r.ext, 8)} ${pad(r.type, 14)} ${padL(r.inB, 8)} ${padL(r.outB, 8)} ${padL(r.cut, 7)} ${r.ok ? 'PASS' : 'FAIL'}`)

if (failures.length) { console.error(`\n✗ minify check FAILED (${failures.length}):`); for (const f of failures) console.error(`  • ${f}`); process.exit(1) }
console.log(`\n✓ all ${rows.length} samples cover every configured minify extension and are deterministic; generic synthetic coverage is explicitly bounded.`)

function languageSynthetic(content) {
  return { content, kind: 'languageSynthetic' }
}

function genericSynthetic(content) {
  return { content, kind: 'genericSynthetic' }
}

function syntheticSampleFor(ext) {
  if (['js', 'jsx', 'mjs', 'cjs'].includes(ext)) {
    return languageSynthetic('// generated fixture\nfunction value(input) {\n  return input + 1;\n}\nvalue(1);\n')
  }
  if (['ts', 'tsx', 'mts', 'cts'].includes(ext)) {
    return languageSynthetic("import type { Input } from './types';\nexport function value(input: Input): string {\n  return String(input);\n}\n")
  }
  if (['json', 'jsonc', 'json5'].includes(ext)) {
    return languageSynthetic('{\n  // generated fixture\n  "name": "octocode",\n  "items": [1, 2, 3],\n}\n')
  }
  if (['md', 'markdown', 'mdx'].includes(ext)) {
    return languageSynthetic('# Generated Fixture\n\n## Details\n\nA wrapped paragraph with enough words to normalize.\n\n<!-- hidden -->\n')
  }
  if (['html', 'htm', 'vue', 'svelte'].includes(ext)) {
    return languageSynthetic('<!-- hidden --><style>.card { margin: 0px; color: red; }</style><script>const value = 1; // comment</script><section>Hi</section>\n')
  }
  if (['ejs', 'erb'].includes(ext)) {
    return languageSynthetic('<%# generated fixture %>\n<section><%= value %></section>\n')
  }
  if (['handlebars', 'hbs', 'mustache'].includes(ext)) {
    return languageSynthetic('{{! generated fixture }}\n<section>{{value}}</section>\n')
  }
  if (['jinja', 'jinja2', 'twig'].includes(ext)) {
    return languageSynthetic('{# generated fixture #}\n{% if value %}<section>{{ value }}</section>{% endif %}\n')
  }
  if (['xml', 'svg', 'xsl', 'xslt'].includes(ext)) {
    return languageSynthetic('<root><!-- hidden --><child value="1">Hi</child></root>\n')
  }
  if (['css', 'scss', 'less', 'sass', 'styl'].includes(ext)) {
    return languageSynthetic('.card {\n  /* generated fixture */\n  margin: 0px;\n  color: red;\n}\n')
  }
  if (['py', 'pyi'].includes(ext)) {
    return languageSynthetic('# generated fixture\ndef value() -> int:\n    return 1\n')
  }
  if (['sh', 'bash', 'zsh', 'fish'].includes(ext)) {
    return languageSynthetic('# generated fixture\nvalue() {\n  echo hi\n}\n')
  }
  if (['sql', 'tsql', 'plsql'].includes(ext)) {
    return languageSynthetic('-- generated fixture\nSELECT id, name FROM users WHERE active = 1;\n')
  }
  if (['yaml', 'yml'].includes(ext)) {
    return languageSynthetic('# generated fixture\nname: octocode\nitems:\n  - one\n  - two\n')
  }
  if (['toml', 'ini', 'cfg', 'conf', 'config', 'env', 'properties'].includes(ext)) {
    return languageSynthetic('# generated fixture\nname = "octocode"\ncount = 2\n')
  }
  if (['vb', 'vbs'].includes(ext)) {
    return languageSynthetic("' generated fixture\nFunction Value()\n  Value = 1\nEnd Function\n")
  }
  if (['erl', 'hrl'].includes(ext)) {
    return languageSynthetic('% generated fixture\nvalue() -> 1.\n')
  }
  if (['hs', 'lhs'].includes(ext)) {
    return languageSynthetic('-- generated fixture\nvalue = 1\n')
  }
  if (['lisp', 'lsp', 'scm', 'rkt', 'clj', 'cljs', 'asm', 'nasm'].includes(ext)) {
    return languageSynthetic('; generated fixture\n(value 1)\n')
  }
  if (['c', 'h', 'cpp', 'hpp', 'cc', 'cxx', 'hh', 'hxx', 'mm'].includes(ext)) {
    return languageSynthetic('/* generated fixture */\nint value(int input) {\n  return input + 1;\n}\n')
  }
  if (ext === 'go') {
    return languageSynthetic('// generated fixture\npackage main\nfunc value(input int) int {\n  return input + 1\n}\n')
  }
  if (['rs', 'rust'].includes(ext)) {
    return languageSynthetic('// generated fixture\nfn value(input: i32) -> i32 {\n    input + 1\n}\n')
  }
  if (ext === 'java') {
    return languageSynthetic('/* generated fixture */\nclass Fixture {\n  int value(int input) { return input + 1; }\n}\n')
  }
  if (ext === 'cs') {
    return languageSynthetic('/* generated fixture */\nclass Fixture {\n  int Value(int input) { return input + 1; }\n}\n')
  }
  if (['kt', 'kts', 'kotlin'].includes(ext)) {
    return languageSynthetic('// generated fixture\nfun value(input: Int): Int {\n  return input + 1\n}\n')
  }
  if (ext === 'swift') {
    return languageSynthetic('// generated fixture\nfunc value(_ input: Int) -> Int {\n  return input + 1\n}\n')
  }
  if (ext === 'dart') {
    return languageSynthetic('// generated fixture\nint value(int input) {\n  return input + 1;\n}\n')
  }
  if (ext === 'groovy') {
    return languageSynthetic('// generated fixture\ndef value(input) {\n  return input + 1\n}\n')
  }
  if (ext === 'gradle') {
    return languageSynthetic('// generated fixture\nplugins {\n  id "java"\n}\n')
  }
  if (['rb', 'rake', 'gemspec', 'ru', 'ruby'].includes(ext)) {
    return languageSynthetic('# generated fixture\ndef value(input)\n  input + 1\nend\n')
  }
  if (ext === 'php') {
    return languageSynthetic('<?php\n// generated fixture\nfunction value($input) {\n  return $input + 1;\n}\n')
  }
  if (['pl', 'pm', 'perl'].includes(ext)) {
    return languageSynthetic('# generated fixture\nsub value {\n  my ($input) = @_;\n  return $input + 1;\n}\n')
  }
  if (ext === 'lua') {
    return languageSynthetic('-- generated fixture\nfunction value(input)\n  return input + 1\nend\n')
  }
  if (['r', 'jl'].includes(ext)) {
    return languageSynthetic('# generated fixture\nvalue <- function(input) {\n  input + 1\n}\n')
  }
  if (['ps1', 'psd1', 'psm1'].includes(ext)) {
    return languageSynthetic('# generated fixture\nfunction Get-Value {\n  param($InputObject)\n  $InputObject + 1\n}\n')
  }
  if (['tf', 'hcl', 'tfvars'].includes(ext)) {
    return languageSynthetic('# generated fixture\nvariable "name" {\n  default = "octocode"\n}\n')
  }
  if (['gql', 'graphql'].includes(ext)) {
    return languageSynthetic('# generated fixture\ntype Query {\n  value(id: ID!): String\n}\n')
  }
  if (ext === 'proto') {
    return languageSynthetic('syntax = "proto3";\n// generated fixture\nmessage Value {\n  string name = 1;\n}\n')
  }
  if (['dockerignore', 'gitignore'].includes(ext)) {
    return languageSynthetic('# generated fixture\nnode_modules/\ndist/\n')
  }
  if (['adb', 'ads'].includes(ext)) {
    return languageSynthetic('-- generated fixture\nprocedure Value is\nbegin\n   null;\nend Value;\n')
  }
  if (['f', 'for', 'f90', 'f95', 'f03', 'f08'].includes(ext)) {
    return languageSynthetic('! generated fixture\nfunction value(input)\n  integer :: input\n  value = input + 1\nend function\n')
  }
  if (['fs', 'fsx'].includes(ext)) {
    return languageSynthetic('// generated fixture\nlet value input = input + 1\n')
  }
  if (['ml', 'mli'].includes(ext)) {
    return languageSynthetic('(* generated fixture *)\nlet value input = input + 1\n')
  }
  if (['vhd', 'vhdl'].includes(ext)) {
    return languageSynthetic('-- generated fixture\nentity value is\nend value;\n')
  }
  if (['wat', 'wast'].includes(ext)) {
    return languageSynthetic(';; generated fixture\n(module (func $value (result i32) i32.const 1))\n')
  }
  if (ext === 'nim') {
    return languageSynthetic('# generated fixture\nproc value(input: int): int =\n  input + 1\n')
  }
  return genericSynthetic('/* generated fixture */\nfunction value(input) {\n  return input + 1;\n}\n\nvalue(1);\n')
}
