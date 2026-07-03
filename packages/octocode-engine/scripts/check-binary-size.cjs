'use strict'

const { existsSync, readdirSync, statSync } = require('fs')
const { join } = require('path')

const root = join(__dirname, '..')
const npmDir = join(root, 'npm')
// Per-binary size budget. The native addon embeds 33 tree-sitter grammars
// (incl. feature-gated large-grammar set), oxc 0.137, lightningcss, and vendored
// pcre2 (for grep -P lookaround/backrefs), so a ~58–60MB per-platform binary is
// expected. Override with OCTOCODE_CONTEXT_BINARY_MAX_BYTES for local tuning.
const maxBytes = Number(process.env.OCTOCODE_CONTEXT_BINARY_MAX_BYTES ?? 60 * 1024 * 1024)

function fail(message) {
  console.error(`binary-size:check failed: ${message}`)
  process.exit(1)
}

if (!existsSync(npmDir)) {
  console.log('binary-size:check skipped: npm platform directory not found')
  process.exit(0)
}

const binaries = []
for (const dirName of readdirSync(npmDir)) {
  const packageDir = join(npmDir, dirName)
  if (!statSync(packageDir).isDirectory()) continue
  for (const entry of readdirSync(packageDir)) {
    if (entry.endsWith('.node')) {
      binaries.push(join(packageDir, entry))
    }
  }
}

if (binaries.length === 0) {
  console.log('binary-size:check skipped: no platform binaries found')
  process.exit(0)
}

const oversized = []
for (const binaryPath of binaries) {
  const size = statSync(binaryPath).size
  if (size > maxBytes) {
    oversized.push(`${binaryPath} (${size} bytes > ${maxBytes})`)
  }
}

if (oversized.length > 0) {
  fail(`native binaries exceed budget:\n${oversized.join('\n')}`)
}

const totalBytes = binaries.reduce((sum, binaryPath) => sum + statSync(binaryPath).size, 0)
console.log(
  `binary-size:check ok: ${binaries.length} binaries, ${totalBytes} total bytes, ${maxBytes} byte per-binary budget`
)
