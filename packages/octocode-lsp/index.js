'use strict'

const { existsSync, readFileSync } = require('fs')
const { join } = require('path')

const packageName = 'octocode-lsp'
const binaryName = 'octocode-lsp'
const { platform, arch } = process

// ── musl detection (three methods, in priority order) ────────────────────────

const isFileMusl = (f) => f.includes('libc.musl-') || f.includes('ld-musl-')

function isMuslFromFilesystem() {
  try {
    return readFileSync('/usr/bin/ldd', 'utf-8').includes('musl')
  } catch {
    return null
  }
}

function isMuslFromReport() {
  let report = null
  if (typeof process.report?.getReport === 'function') {
    process.report.excludeNetwork = true
    report = process.report.getReport()
  }
  if (!report) return null
  if (report.header && report.header.glibcVersionRuntime) return false
  if (Array.isArray(report.sharedObjects)) {
    if (report.sharedObjects.some(isFileMusl)) return true
  }
  return false
}

function isMuslFromChildProcess() {
  try {
    return require('child_process').execSync('ldd --version', { encoding: 'utf8' }).includes('musl')
  } catch {
    return false
  }
}

function isMusl() {
  if (process.platform !== 'linux') return false
  let result = isMuslFromFilesystem()
  if (result === null) result = isMuslFromReport()
  if (result === null) result = isMuslFromChildProcess()
  return !!result
}

// ── Platform key (6 shipped targets) ─────────────────────────────────────────

function getPlatformKey() {
  if (platform === 'darwin') {
    if (arch === 'arm64') return 'darwin-arm64'
    if (arch === 'x64') return 'darwin-x64'
  }

  if (platform === 'linux') {
    const libc = isMusl() ? 'musl' : 'gnu'
    if (arch === 'x64') return `linux-x64-${libc}`
    if (arch === 'arm64' && libc === 'gnu') return 'linux-arm64-gnu'
  }

  if (platform === 'win32' && arch === 'x64') return 'win32-x64-msvc'

  throw new Error(`${packageName} does not ship a native binary for ${platform}-${arch}`)
}

// ── Loader: local dev build first, npm platform sub-package second ────────────

function loadNativeBinding() {
  const key = getPlatformKey()
  const localBinaryPath = join(__dirname, `${binaryName}.${key}.node`)

  if (existsSync(localBinaryPath)) {
    return require(localBinaryPath)
  }

  return require(`${packageName}-${key}`)
}

module.exports = loadNativeBinding()
