#!/usr/bin/env node
// Runs every benchmark feature check via octocode-engine, in order:
//   check-matrix.mjs       — full extension × feature support matrix
//   ast/check-ast.mjs       — structural search + signatures, all grammars
//   lsp/check-lsp.mjs       — language-id + server resolution + native semantics
//   minify/check-minify.mjs — minifier over every configured format
//   cli/check-cli-metadata.mjs — CLI/tool/OQL help + schema metadata surface
//
// Exits non-zero if any check fails. Use individual scripts (yarn ast:check,
// lsp:check, minify:check) to run one in isolation.

import { spawnSync } from 'node:child_process'
import { mkdirSync, writeFileSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const resultsDir = join(here, '..', 'results', 'ci')
const latestPath = join(resultsDir, 'latest.md')
const checks = [
  ['MATRIX', 'matrix:check', join(here, 'check-matrix.mjs')],
  ['AST', 'ast:check', join(here, 'ast', 'check-ast.mjs')],
  ['LSP', 'lsp:check', join(here, 'lsp', 'check-lsp.mjs')],
  ['MINIFY', 'minify:check', join(here, 'minify', 'check-minify.mjs')],
  ['CLI_METADATA', 'cli:check', join(here, 'cli', 'check-cli-metadata.mjs')],
]

function renderLatest(results, generatedAt) {
  const rows = results
    .map(result => {
      const status =
        result.status === 0 ? 'PASS' : `FAIL (exit ${result.status})`
      return `| ${result.name} | ${status} | ${generatedAt} |`
    })
    .join('\n')

  return `# CI Benchmark Results — Latest Run

> Populated by \`yarn benchmark\`. Re-run to refresh.
>
> For full timestamped run history, see \`output/<benchmark-name>-<YYYYMMDDTHHMMSSZ>/\`.

## Suite: matrix + AST + LSP + minify + CLI metadata

| Check | Status | Last run |
|-------|--------|----------|
${rows}
`
}

let failed = 0
const results = []
const generatedAt = new Date().toISOString()

for (const [label, name, script] of checks) {
  const r = spawnSync(process.execPath, [script], { stdio: 'inherit' })
  const status = r.status ?? 1
  results.push({ name, status })
  if (status !== 0) { failed++; console.error(`\n[${label}] FAILED (exit ${status})`) }
}

mkdirSync(resultsDir, { recursive: true })
writeFileSync(latestPath, renderLatest(results, generatedAt))

console.log('\n' + '='.repeat(60))
if (failed) { console.error(`✗ ${failed}/${checks.length} benchmark checks FAILED`); process.exit(1) }
console.log(
  `✓ all ${checks.length} benchmark checks passed (matrix + AST + LSP + minify + CLI metadata)`
)
