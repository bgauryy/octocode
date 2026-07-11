#!/usr/bin/env node
// Optional external comparison against the ast-grep CLI.
//
// This script intentionally shells out to an installed `ast-grep` binary.
// Octocode does not link ast-grep crates or packages.

import { spawnSync } from 'node:child_process'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { performance } from 'node:perf_hooks'

const AST_GREP_BIN = process.env.AST_GREP_BIN || 'ast-grep'

const INSTALL_GUIDANCE = `ast-grep CLI is required for this optional comparison.

Install one of:
  brew install ast-grep
  npm install --global @ast-grep/cli
  pip install ast-grep-cli
  cargo install ast-grep --locked
  cargo binstall ast-grep

Then verify:
  ast-grep --version

You can also point to a custom binary:
  AST_GREP_BIN=/path/to/ast-grep yarn ast:compare
`

const probe = spawnSync(AST_GREP_BIN, ['--version'], { encoding: 'utf8' })
if (probe.error?.code === 'ENOENT') {
  console.error(INSTALL_GUIDANCE)
  process.exit(2)
}
if (probe.status !== 0) {
  console.error(`Could not run ${AST_GREP_BIN} --version`)
  console.error(probe.stderr || probe.stdout)
  process.exit(probe.status || 1)
}

const { engine } = await import('../_engine.mjs')

const CASES = [
  {
    name: 'typescript-call',
    ext: 'ts',
    content: 'const a = foo(bar);\nconst b = foo(baz);\n',
    pattern: 'foo($X)',
  },
  {
    name: 'javascript-multi-capture',
    ext: 'js',
    content: 'log(1, 2, 3);\n',
    pattern: 'log($$$ARGS)',
  },
  {
    name: 'python-call',
    ext: 'py',
    content: 'print(value)\n',
    pattern: 'print($X)',
  },
  {
    name: 'rust-call-kind',
    ext: 'rs',
    content: 'fn f() { let _ = g(y); }\n',
    kind: 'call_expression',
  },
  {
    name: 'css-declaration',
    ext: 'css',
    content: '.btn { color: red; }\n',
    pattern: '.btn { color: $C; }',
  },
]

function parseAstGrepJson(output) {
  const text = output.trim()
  if (!text) return []
  if (text.startsWith('[')) return JSON.parse(text)
  return text
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => JSON.parse(line))
}

function runAstGrep(testCase) {
  const dir = mkdtempSync(join(tmpdir(), 'octocode-ast-grep-'))
  const file = join(dir, `case.${testCase.ext}`)
  writeFileSync(file, testCase.content)

  const args = ['run', '--json=stream']
  if (testCase.pattern) args.push('--pattern', testCase.pattern)
  if (testCase.kind) args.push('--kind', testCase.kind)
  args.push(file)

  const started = performance.now()
  const result = spawnSync(AST_GREP_BIN, args, { encoding: 'utf8' })
  const duration = performance.now() - started
  rmSync(dir, { recursive: true, force: true })

  if (result.error) throw result.error
  if (result.status !== 0 && !result.stdout.trim()) {
    throw new Error(result.stderr.trim() || `${AST_GREP_BIN} exited ${result.status}`)
  }

  return {
    duration,
    matches: parseAstGrepJson(result.stdout).map((match) => match.text),
  }
}

async function runOctocode(testCase) {
  const started = performance.now()
  const rule = testCase.kind ? `rule:\n  kind: ${testCase.kind}\n` : null
  const matches = await engine.structuralSearch(
    testCase.content,
    `case.${testCase.ext}`,
    testCase.pattern || null,
    rule,
  )
  const duration = performance.now() - started

  return {
    duration,
    matches: matches.map((match) => match.text),
  }
}

function normalized(matches) {
  return matches.map((text) => text.trim()).sort()
}

const rows = []
const failures = []

for (const testCase of CASES) {
  const ast = runAstGrep(testCase)
  const octo = await runOctocode(testCase)
  const astTexts = normalized(ast.matches)
  const octoTexts = normalized(octo.matches)
  const ok = JSON.stringify(astTexts) === JSON.stringify(octoTexts)

  if (!ok) {
    failures.push(
      `${testCase.name}: ast-grep=${JSON.stringify(astTexts)} octocode=${JSON.stringify(octoTexts)}`,
    )
  }

  rows.push({
    name: testCase.name,
    ext: testCase.ext,
    matches: `${octo.matches.length}/${ast.matches.length}`,
    astMs: ast.duration.toFixed(2),
    octoMs: octo.duration.toFixed(2),
    ok,
  })
}

const pad = (value, width) => String(value).padEnd(width)
console.log(`\nast-grep CLI comparison using ${AST_GREP_BIN} (${probe.stdout.trim()})`)
console.log(`${pad('Case', 26)} ${pad('ext', 5)} ${pad('octo/ast', 9)} ${pad('ast ms', 8)} ${pad('octo ms', 8)} status`)
console.log('-'.repeat(70))
for (const row of rows) {
  console.log(
    `${pad(row.name, 26)} ${pad(row.ext, 5)} ${pad(row.matches, 9)} ${pad(row.astMs, 8)} ${pad(row.octoMs, 8)} ${row.ok ? 'PASS' : 'FAIL'}`,
  )
}

if (failures.length) {
  console.error(`\nComparison failed (${failures.length}):`)
  for (const failure of failures) console.error(`  - ${failure}`)
  process.exit(1)
}

console.log(`\nPASS: ${CASES.length} CLI-compatible structural cases matched ast-grep output.`)
