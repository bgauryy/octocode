#!/usr/bin/env node
// Runs every benchmark feature check via octocode-engine, in order:
//   ast/check-ast.mjs       — structural search + signatures, all grammars
//   lsp/check-lsp.mjs       — language-id + server resolution + native semantics
//   minify/check-minify.mjs — minifier over every configured format
//   cli/check-cli-metadata.mjs — CLI/tool/OQL help + schema metadata surface
//
// Exits non-zero if any check fails. Use individual scripts (yarn ast:check,
// lsp:check, minify:check) to run one in isolation.

import { spawnSync } from 'node:child_process'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const checks = [
  ['MATRIX', join(here, 'check-matrix.mjs')],
  ['AST', join(here, 'ast', 'check-ast.mjs')],
  ['LSP', join(here, 'lsp', 'check-lsp.mjs')],
  ['MINIFY', join(here, 'minify', 'check-minify.mjs')],
  ['CLI_METADATA', join(here, 'cli', 'check-cli-metadata.mjs')],
]

let failed = 0
for (const [label, script] of checks) {
  const r = spawnSync(process.execPath, [script], { stdio: 'inherit' })
  if (r.status !== 0) { failed++; console.error(`\n[${label}] FAILED (exit ${r.status})`) }
}

console.log('\n' + '='.repeat(60))
if (failed) { console.error(`✗ ${failed}/${checks.length} benchmark checks FAILED`); process.exit(1) }
console.log(
  `✓ all ${checks.length} benchmark checks passed (matrix + AST + LSP + minify + CLI metadata)`
)
