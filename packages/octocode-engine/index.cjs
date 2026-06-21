'use strict'

const { existsSync } = require('fs')
const { join } = require('path')

const packageName = '@octocodeai/octocode-engine'
const binaryName = 'octocode-engine'
const { platform, arch } = process

const { readFileSync } = require('fs')

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

function loadNativeBinding() {
  const key = getPlatformKey()
  const candidates = [
    join(__dirname, `${binaryName}.${key}.node`),
    join(__dirname, 'runtime', 'engine', `${binaryName}.${key}.node`),
    join(__dirname, '..', 'runtime', 'engine', `${binaryName}.${key}.node`),
    join(__dirname, '..', '..', 'runtime', 'engine', `${binaryName}.${key}.node`),
  ]

  const loadErrors = []
  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      try {
        return require(candidate)
      } catch (err) {
        // The .node file exists but the host refused to load it — most often a
        // hardened/sandboxed runtime (e.g. an editor/app-embedded Node) that
        // blocks native addons, or an ABI/arch mismatch. Distinguish this from
        // "binary missing" so callers don't report a generic tool failure.
        loadErrors.push(`${candidate}: ${err && err.message ? err.message : err}`)
      }
    }
  }

  try {
    return require(`${packageName}-${key}`)
  } catch (err) {
    loadErrors.push(`${packageName}-${key}: ${err && err.message ? err.message : err}`)
  }

  const detail = loadErrors.length
    ? `\nNative load attempts:\n  - ${loadErrors.join('\n  - ')}`
    : `\nNo native binary found for ${key} (looked in ${candidates.length} locations and the ${packageName}-${key} package).`
  const error = new Error(
    `@octocodeai/octocode-engine: could not load the native ${binaryName} addon for ${key}.` +
      detail +
      `\nIf a .node file exists above but failed to load, the current Node runtime is likely sandboxed and rejects ` +
      `native addons (e.g. an editor/app-embedded Node). Re-run with system Node (\`which node\`).`
  )
  error.code = 'OCTOCODE_ENGINE_NATIVE_LOAD_FAILED'
  throw error
}

const nativeBinding = loadNativeBinding()

nativeBinding.MINIFY_CONFIG = nativeBinding.getMINIFY_CONFIG()
nativeBinding.SUPPORTED_SIGNATURE_EXTENSIONS = Object.freeze(
  nativeBinding.getSupportedSignatureExtensions().sort()
)
nativeBinding.SUPPORTED_STRUCTURAL_EXTENSIONS = Object.freeze(
  nativeBinding.getSupportedStructuralExtensions().sort()
)

module.exports = nativeBinding
