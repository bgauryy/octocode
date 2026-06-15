'use strict'

const { existsSync, readdirSync, statSync } = require('fs')
const { join } = require('path')

const root = join(__dirname, '..')
const npmDir = join(root, 'npm')
const maxBytes = Number(process.env.OCTOCODE_CONTEXT_BINARY_MAX_BYTES ?? 45 * 1024 * 1024)

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
