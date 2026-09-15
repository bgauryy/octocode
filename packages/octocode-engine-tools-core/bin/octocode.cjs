#!/usr/bin/env node
/**
 * Platform-selecting shim for the native `octocode` CLI binary.
 *
 * npm/yarn installs only the matching platform package via optionalDependencies
 * (cpu + os guards). This shim resolves that package at runtime, finds the
 * compiled binary, and spawns it — passing all arguments and stdio through.
 */
'use strict'

const { spawnSync } = require('child_process')
const { join } = require('path')
const { existsSync, statSync } = require('fs')

// ── platform detection ────────────────────────────────────────────────────────

const isWindows = process.platform === 'win32'

/**
 * Detect musl vs gnu on Linux by checking for Alpine's marker file.
 * Falls back to gnu (glibc) on all other Linux distributions.
 */
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
    `octocode: unsupported platform '${key}'.\n` +
    `Supported: ${Object.keys(PLATFORM_MAP).join(', ')}`
  )
  process.exit(1)
}

// ── resolve platform package ──────────────────────────────────────────────────

const pkgName = `@octocodeai/octocode-native-${platformSuffix}`
const binaryName = isWindows ? 'octocode.exe' : 'octocode'

let pkgDir
try {
  // require.resolve finds the package.json; strip it to get the dir.
  pkgDir = require.resolve(`${pkgName}/package.json`).replace(/[\/\\]package\.json$/, '')
} catch {
  console.error(
    `octocode: platform package '${pkgName}' is not installed.\n` +
    `This usually means the optional dependency was skipped.\n` +
    `Try: npm install ${pkgName}`
  )
  process.exit(1)
}

const binaryPath = join(pkgDir, binaryName)

if (!existsSync(binaryPath)) {
  console.error(
    `octocode: binary not found at '${binaryPath}'.\n` +
    `Rebuild: yarn workspace @octocodeai/octocode-native build:${platformSuffix}`
  )
  process.exit(1)
}

// ── spawn ─────────────────────────────────────────────────────────────────────

const result = spawnSync(binaryPath, process.argv.slice(2), {
  stdio: 'inherit',
  windowsHide: false,
})

if (result.error) {
  console.error(`octocode: failed to start binary: ${result.error.message}`)
  process.exit(1)
}

process.exit(result.status ?? 1)
