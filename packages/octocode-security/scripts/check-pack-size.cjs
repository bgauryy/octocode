/**
 * CI guard for the main npm package. Native binaries are distributed through
 * optional per-platform packages, so the root package must not publish local
 * `.node` artifacts.
 */
'use strict'

const { spawnSync } = require('child_process')

const MAX_PACKED_BYTES = Number(process.env.OCTOCODE_SECURITY_PACK_MAX_BYTES ?? 1_000_000)
const MAX_UNPACKED_BYTES = Number(process.env.OCTOCODE_SECURITY_UNPACKED_MAX_BYTES ?? 2_000_000)

function fail(message) {
  console.error(`pack:check failed: ${message}`)
  process.exit(1)
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
  packs = JSON.parse(result.stdout)
} catch (error) {
  process.stdout.write(result.stdout)
  fail(`npm pack did not return JSON: ${error.message}`)
}

const pack = packs?.[0]
if (!pack) {
  fail('npm pack returned no package metadata')
}

const nativeFiles = pack.files
  .map((file) => file.path)
  .filter((filePath) => filePath.endsWith('.node'))

if (nativeFiles.length > 0) {
  fail(`main package includes native binaries: ${nativeFiles.join(', ')}`)
}

if (pack.size > MAX_PACKED_BYTES) {
  fail(`packed size ${pack.size} exceeds budget ${MAX_PACKED_BYTES}`)
}

if (pack.unpackedSize > MAX_UNPACKED_BYTES) {
  fail(`unpacked size ${pack.unpackedSize} exceeds budget ${MAX_UNPACKED_BYTES}`)
}

console.log(
  `pack:check ok: ${pack.files.length} files, ${pack.size} packed bytes, ${pack.unpackedSize} unpacked bytes`
)
