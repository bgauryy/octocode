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

const { statSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } = require('fs')
const { join } = require('path')
const { tmpdir } = require('os')
const { spawnSync } = require('child_process')
const { getPlatformSuffix } = require('../bin/platform.cjs')

const cwd = process.cwd()
const pkg = require(join(cwd, 'package.json'))
const isWindows = pkg.os && pkg.os.includes('win32')
const ext = isWindows ? '.exe' : ''
const packageSuffix = pkg.name.slice('@octocodeai/octocode-native-'.length)
const binaries = [`octocode${ext}`, `octocode-regex-worker${ext}`, `octocode-native.${packageSuffix}.node`]

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

const nativePlatform = getPlatformSuffix() === packageSuffix
if (!nativePlatform) {
  console.log(
    `prepublishOnly: ${pkg.name} executable smoke test skipped on ${process.platform}-${process.arch}`
  )
  process.exit(0)
}

const addon = require(join(cwd, `octocode-native.${packageSuffix}.node`))
if (typeof addon.NativeRuntime !== 'function') {
  console.error(`prepublishOnly: ${pkg.name} native addon does not expose NativeRuntime`)
  process.exit(1)
}

const octocode = join(cwd, `octocode${ext}`)
const run = (args, options = {}) => spawnSync(octocode, args, {
  encoding: 'utf8',
  timeout: 20_000,
  ...options,
})
const parseOutput = result => {
  try {
    return JSON.parse(result.stdout)
  } catch {
    return null
  }
}
const fail = message => {
  console.error(`prepublishOnly: ${pkg.name} ${message}`)
  process.exit(1)
}

const version = run(['--version'])
if (version.status !== 0 || !version.stdout.includes(pkg.version)) {
  fail(
    `binary version mismatch; expected ${pkg.version}, got ${version.stdout.trim() || version.stderr.trim()}`
  )
}

const catalog = run(['tools', '--json'], {
  env: { ...process.env, ENABLE_CLONE: 'false' },
})
const parsed = parseOutput(catalog)
const tools = parsed && Array.isArray(parsed.tools) ? parsed.tools : []
const required = ['localSearch', 'astSearch', 'lspSearch']
if (catalog.status !== 0 || required.some(name => !tools.some(tool => tool.name === name))) {
  fail(`binary does not expose the current native tool catalog: ${catalog.stderr.trim()}`)
}

const schema = run(['tools', '--scheme', 'lspSearch', '--compact'])
const schemaText = JSON.stringify(parseOutput(schema)?.querySchema ?? {})
if (
  schema.status !== 0 ||
  !schemaText.includes('"position"') ||
  !schemaText.includes('"snapshot"') ||
  !schemaText.includes('"contextLines"')
) {
  fail('binary does not embed the current lspSearch contract')
}

const fixture = mkdtempSync(join(tmpdir(), 'octocode-native-smoke-'))
try {
  const home = join(fixture, 'home')
  mkdirSync(home)
  const source = join(fixture, 'source.rs')
  const notes = join(fixture, 'notes.md')
  writeFileSync(source, 'pub fn packaged_binary_needle() {}\n')
  writeFileSync(notes, '# packaged binary\n')
  const env = {
    ...process.env,
    ALLOWED_PATHS: fixture,
    WORKSPACE_ROOT: fixture,
    OCTOCODE_HOME: home,
    ENABLE_LOCAL: 'true',
    ENABLE_CLONE: 'false',
    OCTOCODE_ENABLE_STATS: 'false',
  }

  const local = run([
    'tools',
    'localSearch',
    JSON.stringify({ path: fixture, searchText: 'packaged_binary_needle', regex: 'literal' }),
    '--json',
    '--compact',
  ], { env })
  if (local.status !== 0 || !local.stdout.includes('source.rs')) {
    fail(`localSearch smoke failed: ${local.stderr.trim()}`)
  }

  const ast = run([
    'tools',
    'astSearch',
    JSON.stringify({ operation: 'files', path: fixture }),
    '--json',
    '--compact',
  ], { env })
  if (ast.status !== 0 || !ast.stdout.includes('source.rs')) {
    fail(`astSearch smoke failed: ${ast.stderr.trim()}`)
  }

  const lsp = run([
    'tools',
    'lspSearch',
    JSON.stringify({ operation: 'documentSymbols', uri: notes }),
    '--json',
    '--compact',
  ], { env })
  const lspResult = parseOutput(lsp)
  if (
    lsp.status !== 5 ||
    lspResult?.results?.[0]?.status !== 'error' ||
    lspResult?.results?.[0]?.data?.errorCode !== 'lsp.serverUnavailable'
  ) {
    fail(`lspSearch typed-failure smoke failed: ${lsp.stderr.trim()}`)
  }
} finally {
  rmSync(fixture, { recursive: true, force: true })
}

console.log(`prepublishOnly: ${pkg.name} executable smoke test \u2713`)
