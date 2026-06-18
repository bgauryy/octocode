/**
 * Publish guard for the per-platform native packages.
 *
 * The root loader package ships NO `.node` — each platform binary is delivered
 * through its own `npm/<platform>/` optional dependency. A host-only build
 * (`napi build --platform` with no `--target`) populates just ONE of those dirs
 * and silently skips the other five, so publishing without a full `build:all`
 * would ship empty platform packages and break installs on those platforms.
 *
 * This script asserts that EVERY declared platform package contains exactly one
 * non-empty `.node`, and that `npm pack` actually includes it. Run it from the
 * package `verify` script and before any publish.
 */
'use strict'

const { spawnSync } = require('child_process')
const { existsSync, statSync, readdirSync } = require('fs')
const { join } = require('path')

const root = join(__dirname, '..')
const pkg = require(join(root, 'package.json'))
const binaryName = pkg.napi && pkg.napi.binaryName
const optionalDeps = Object.keys(pkg.optionalDependencies || {})

function fail(message) {
  console.error(`check-platform-binaries failed: ${message}`)
  process.exit(1)
}

if (!binaryName) {
  fail('package.json is missing napi.binaryName')
}

const npmDir = join(root, 'npm')
if (!existsSync(npmDir)) {
  fail(`missing npm/ directory at ${npmDir}`)
}

const platformDirs = readdirSync(npmDir, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)

if (platformDirs.length === 0) {
  fail('no platform package directories found under npm/')
}

if (optionalDeps.length && platformDirs.length !== optionalDeps.length) {
  fail(
    `platform dir count (${platformDirs.length}) does not match ` +
      `optionalDependencies count (${optionalDeps.length})`
  )
}

const npmBin = process.platform === 'win32' ? 'npm.cmd' : 'npm'
const problems = []

for (const triple of platformDirs) {
  const packageDir = join(npmDir, triple)
  const binaryFile = `${binaryName}.${triple}.node`
  const binaryPath = join(packageDir, binaryFile)

  if (!existsSync(binaryPath)) {
    problems.push(
      `npm/${triple}: missing ${binaryFile} ` +
        `(run "yarn build:all" / the matching per-target build before publishing)`
    )
    continue
  }

  const size = statSync(binaryPath).size
  if (size === 0) {
    problems.push(`npm/${triple}: ${binaryFile} is empty (0 bytes)`)
    continue
  }

  const result = spawnSync(npmBin, ['pack', '--dry-run', '--json'], {
    cwd: packageDir,
    encoding: 'utf8',
  })
  if (result.status !== 0) {
    problems.push(`npm/${triple}: npm pack exited with ${result.status}`)
    continue
  }

  let nodes
  try {
    const pack = JSON.parse(result.stdout)[0]
    nodes = pack.files
      .map((file) => file.path)
      .filter((filePath) => filePath.endsWith('.node'))
  } catch (error) {
    problems.push(`npm/${triple}: could not parse npm pack output: ${error.message}`)
    continue
  }

  if (nodes.length !== 1) {
    problems.push(
      `npm/${triple}: expected exactly 1 .node in the tarball, got ${nodes.length}` +
        (nodes.length ? ` (${nodes.join(', ')})` : '')
    )
    continue
  }

  console.log(`npm/${triple}: ${nodes[0]} (${size} bytes) ✓`)
}

if (problems.length) {
  console.error('\nPlatform binary check found problems:')
  for (const problem of problems) {
    console.error(`  - ${problem}`)
  }
  process.exit(1)
}

console.log(`\nAll ${platformDirs.length} platform packages contain a valid .node ✓`)
