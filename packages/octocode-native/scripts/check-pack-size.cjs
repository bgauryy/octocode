/**
 * CI guard for the main npm package. Native binaries are distributed through
 * optional per-platform packages, so the root package must stay a small JS/type
 * loader and must never include a `.node` artifact.
 */
'use strict'

const { spawnSync } = require('child_process')
const { readFileSync } = require('fs')
const { join } = require('path')

const MAX_PACKED_BYTES = Number(process.env.OCTOCODE_CONTEXT_PACK_MAX_BYTES ?? 1_000_000)
const MAX_UNPACKED_BYTES = Number(process.env.OCTOCODE_CONTEXT_UNPACKED_MAX_BYTES ?? 2_000_000)

function fail(message) {
  console.error(`pack:check failed: ${message}`)
  process.exit(1)
}

function parseNpmPackJson(stdout) {
  const jsonStart = stdout.indexOf('[')
  if (jsonStart === -1) {
    throw new Error('no JSON array found in npm pack output')
  }
  return JSON.parse(stdout.slice(jsonStart))
}

const npmBin = process.platform === 'win32' ? 'npm.cmd' : 'npm'
const result = spawnSync(npmBin, ['pack', '--dry-run', '--json'], {
  cwd: __dirname + '/..',
  encoding: 'utf8',
})

if (result.error) {
  fail(result.error.message)
}

if (result.status !== 0) {
  process.stderr.write(result.stderr)
  fail(`npm pack exited with ${result.status}`)
}

let packs
try {
  packs = parseNpmPackJson(result.stdout)
} catch (error) {
  process.stdout.write(result.stdout)
  fail(`npm pack did not return JSON: ${error.message}`)
}

const pack = packs?.[0]
if (!pack) {
  fail('npm pack returned no package metadata')
}

const packedFiles = new Set(pack.files.map((file) => file.path))
const nativeFiles = [...packedFiles].filter((filePath) => filePath.endsWith('.node'))

if (nativeFiles.length > 0) {
  fail(`main package includes native binaries: ${nativeFiles.join(', ')}`)
}

if (pack.size > MAX_PACKED_BYTES) {
  fail(`packed size ${pack.size} exceeds budget ${MAX_PACKED_BYTES}`)
}

if (pack.unpackedSize > MAX_UNPACKED_BYTES) {
  fail(`unpacked size ${pack.unpackedSize} exceeds budget ${MAX_UNPACKED_BYTES}`)
}

const manifest = JSON.parse(readFileSync(join(__dirname, '..', 'package.json'), 'utf8'))
const exportedTargets = new Set()
for (const conditions of Object.values(manifest.exports ?? {})) {
  if (typeof conditions === 'string') {
    exportedTargets.add(conditions)
    continue
  }
  for (const target of Object.values(conditions)) {
    if (typeof target === 'string') exportedTargets.add(target)
  }
}
const missingExportTargets = [...exportedTargets]
  .map((target) => target.replace(/^\.\//, ''))
  .filter((target) => !packedFiles.has(target))
  .sort()
if (missingExportTargets.length > 0) {
  fail(`package exports missing from tarball: ${missingExportTargets.join(', ')}`)
}

console.log(
  `pack:check ok: ${pack.files.length} files, ${pack.size} packed bytes, ${pack.unpackedSize} unpacked bytes`
)
