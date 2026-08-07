'use strict'

/**
 * Post-publish guard: confirms the main engine package AND every declared
 * optionalDependency (the per-platform native binaries) actually landed on
 * the npm registry at the SAME version — not just that local files agree.
 *
 * check-version-consistency.cjs only compares files on disk, so a publish
 * where one of the 6 platform `npm publish` calls silently fails (network
 * blip, npm 2FA prompt, auth hiccup) still passes it, ships the main
 * package fine, and leaves every install crashing at runtime with
 * OCTOCODE_ENGINE_NATIVE_LOAD_FAILED. Run this AFTER publishing, before
 * announcing a release.
 */

const { readFileSync } = require('fs')
const { join } = require('path')
const { execFileSync } = require('child_process')

const root = join(__dirname, '..')
const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
const version = pkg.version
const optionalDependencies = pkg.optionalDependencies ?? {}

function publishedVersions(name) {
  try {
    const raw = execFileSync(
      'npm',
      ['view', name, 'versions', '--json'],
      { encoding: 'utf8' }
    )
    const parsed = JSON.parse(raw)
    return Array.isArray(parsed) ? parsed : [parsed]
  } catch {
    return []
  }
}

const targets = [pkg.name, ...Object.keys(optionalDependencies)]
const problems = []

for (const name of targets) {
  const versions = publishedVersions(name)
  if (!versions.includes(version)) {
    problems.push(`${name}@${version} is NOT on the npm registry`)
  } else {
    console.log(`${name}@${version} ✓`)
  }
}

if (problems.length) {
  console.error(`\nregistry:check failed — publish is INCONSISTENT at ${version}:`)
  for (const problem of problems) {
    console.error(`  - ${problem}`)
  }
  console.error(
    '\nEvery install of the main package will crash at runtime ' +
      '(OCTOCODE_ENGINE_NATIVE_LOAD_FAILED) until the missing package(s) ' +
      'above are published at the same version. Publish them now, or bump ' +
      'to a new version and republish everything together.'
  )
  process.exit(1)
}

console.log(`\nregistry:check ok: ${pkg.name} and all ${Object.keys(optionalDependencies).length} platform packages are live at ${version}`)
