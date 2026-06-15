'use strict'

const { existsSync } = require('fs')
const { join } = require('path')

const packageName = '@octocodeai/octocode-context-utils'
const binaryName = 'octocode-context-utils'
const { platform, arch } = process

function isMusl() {
  const report = process.report?.getReport?.()
  const header = report && typeof report === 'object' ? report.header : undefined
  return !(header && typeof header === 'object' && 'glibcVersionRuntime' in header)
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
  const localBinaryPath = join(__dirname, `${binaryName}.${key}.node`)

  if (existsSync(localBinaryPath)) {
    return require(localBinaryPath)
  }

  return require(`${packageName}-${key}`)
}

const nativeBinding = loadNativeBinding()

nativeBinding.MINIFY_CONFIG = nativeBinding.getMINIFY_CONFIG()
nativeBinding.SUPPORTED_SIGNATURE_EXTENSIONS = Object.freeze(
  nativeBinding.getSupportedSignatureExtensions().sort()
)

module.exports = nativeBinding
