#!/usr/bin/env node
/**
 * Platform-selecting shim for the native `octocode-regex-worker` binary.
 * See bin/octocode.cjs for the full explanation.
 */
'use strict'

const { spawnSync } = require('child_process')
const { join } = require('path')
const { existsSync, statSync } = require('fs')

const isWindows = process.platform === 'win32'

function linuxLibc() {
  try {
    statSync('/etc/alpine-release')
    return 'musl'
  } catch {
    return 'gnu'
  }
}

const PLATFORM_MAP = {
  'darwin-arm64':  'darwin-arm64',
  'darwin-x64':    'darwin-x64',
  'linux-arm64':   `linux-arm64-${linuxLibc()}`,
  'linux-x64':     `linux-x64-${linuxLibc()}`,
  'win32-x64':     'win32-x64-msvc',
}

const key = `${process.platform}-${process.arch}`
const platformSuffix = PLATFORM_MAP[key]

if (!platformSuffix) {
  console.error(
    `octocode-regex-worker: unsupported platform '${key}'.\n` +
    `Supported: ${Object.keys(PLATFORM_MAP).join(', ')}`
  )
  process.exit(1)
}

const pkgName = `@octocodeai/octocode-native-${platformSuffix}`
const binaryName = isWindows ? 'octocode-regex-worker.exe' : 'octocode-regex-worker'

let pkgDir
try {
  pkgDir = require.resolve(`${pkgName}/package.json`).replace(/[\/\\]package\.json$/, '')
} catch {
  console.error(
    `octocode-regex-worker: platform package '${pkgName}' is not installed.\n` +
    `Try: npm install ${pkgName}`
  )
  process.exit(1)
}

const binaryPath = join(pkgDir, binaryName)

if (!existsSync(binaryPath)) {
  console.error(`octocode-regex-worker: binary not found at '${binaryPath}'.`)
  process.exit(1)
}

const result = spawnSync(binaryPath, process.argv.slice(2), {
  stdio: 'inherit',
  windowsHide: false,
})

if (result.error) {
  console.error(`octocode-regex-worker: failed to start binary: ${result.error.message}`)
  process.exit(1)
}

process.exit(result.status ?? 1)
