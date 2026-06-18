/**
 * prepublishOnly gate for a single platform package.
 *
 * Runs from inside the platform package dir (npm/<triple>/) right before
 * `npm publish`. Aborts the publish if the `.node` named by this package's
 * `main` field is missing or empty — preventing an empty platform package
 * (e.g. from a host-only build) from ever reaching the registry.
 *
 * Not listed in any package `files`, so it is never included in a tarball.
 */
'use strict'

const { statSync } = require('fs')
const { join } = require('path')

const cwd = process.cwd()
const pkg = require(join(cwd, 'package.json'))
const binary = pkg.main

if (!binary || !binary.endsWith('.node')) {
  console.error(`prepublishOnly: ${pkg.name} has no .node "main" field`)
  process.exit(1)
}

let size
try {
  size = statSync(join(cwd, binary)).size
} catch {
  console.error(
    `prepublishOnly: ${pkg.name} is missing ${binary} — ` +
      `build it (yarn workspace <root> build:all) before publishing`
  )
  process.exit(1)
}

if (size === 0) {
  console.error(`prepublishOnly: ${pkg.name} ${binary} is empty (0 bytes)`)
  process.exit(1)
}

console.log(`prepublishOnly: ${pkg.name} ${binary} (${size} bytes) ✓`)
