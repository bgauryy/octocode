'use strict'

const { readFileSync, readdirSync, existsSync } = require('fs')
const { join } = require('path')

const root = join(__dirname, '..')

function fail(message) {
  console.error(`version:check failed: ${message}`)
  process.exit(1)
}

function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'))
}

const rootPackage = readJson(join(root, 'package.json'))
const cargoToml = readFileSync(join(root, 'Cargo.toml'), 'utf8')
const cargoVersion = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1]

if (!cargoVersion) {
  fail('Cargo.toml package version not found')
}

if (cargoVersion !== rootPackage.version) {
  fail(`Cargo.toml version ${cargoVersion} does not match package.json version ${rootPackage.version}`)
}

const optionalDependencies = rootPackage.optionalDependencies ?? {}
for (const [name, version] of Object.entries(optionalDependencies)) {
  if (version !== rootPackage.version) {
    fail(`${name} optional dependency version ${version} does not match ${rootPackage.version}`)
  }
}

const npmDir = join(root, 'npm')
if (existsSync(npmDir)) {
  for (const dirName of readdirSync(npmDir)) {
    const packagePath = join(npmDir, dirName, 'package.json')
    if (!existsSync(packagePath)) continue

    const platformPackage = readJson(packagePath)
    if (platformPackage.version !== rootPackage.version) {
      fail(`${platformPackage.name} version ${platformPackage.version} does not match ${rootPackage.version}`)
    }
  }
}

console.log(`version:check ok: npm, Cargo, and optional package versions are ${rootPackage.version}`)
