/**
 * prepublishOnly gate for a single platform package.
 *
 * Runs from inside the platform package dir (npm/<platform>/) right before
 * `npm publish`. Aborts the publish if either the octocode or
 * octocode-regex-worker binary is missing or empty — preventing an empty
 * platform package from ever reaching the registry.
 *
 * Not listed in any package `files`, so it is never included in a tarball.
 */
'use strict'

const { statSync } = require('fs')
const { join } = require('path')

const cwd = process.cwd()
const pkg = require(join(cwd, 'package.json'))
const isWindows = pkg.os && pkg.os.includes('win32')
const ext = isWindows ? '.exe' : ''
const binaries = [`octocode${ext}`, `octocode-regex-worker${ext}`]

for (const name of binaries) {
  let size
  try {
    size = statSync(join(cwd, name)).size
  } catch {
    console.error(
      `prepublishOnly: ${pkg.name} is missing '${name}' \u2014 ` +
        `build it (yarn workspace @octocodeai/octocode-native build:all) before publishing`
    )
    process.exit(1)
  }

  if (size === 0) {
    console.error(`prepublishOnly: ${pkg.name} '${name}' is empty (0 bytes)`)
    process.exit(1)
  }

  console.log(`prepublishOnly: ${pkg.name} ${name} (${size} bytes) \u2713`)
}
