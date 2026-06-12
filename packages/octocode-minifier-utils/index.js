'use strict'

const { existsSync } = require('fs')
const { join }       = require('path')
const { platform, arch } = process

// ── musl detection (required for Alpine / musl Linux) ────────────────────────
function isMusl() {
  try {
    const { glibcVersionRuntime } = process.report?.getReport()?.header ?? {}
    return !glibcVersionRuntime
  } catch {
    return true
  }
}

// ── platform → .node filename map ───────────────────────────────────────────
const name = 'octocode-minifier-utils'
const triples = {
  'darwin-arm64':   `${name}.darwin-arm64.node`,
  'darwin-x64':     `${name}.darwin-x64.node`,
  'linux-arm64':    `${name}.linux-arm64-gnu.node`,
  'linux-x64-gnu':  `${name}.linux-x64-gnu.node`,
  'linux-x64-musl': `${name}.linux-x64-musl.node`,
  'win32-x64':      `${name}.win32-x64-msvc.node`,
}

const key = platform === 'linux'
  ? `linux-${arch}-${isMusl() ? 'musl' : 'gnu'}`
  : `${platform}-${arch}`

let nativeBinding = null
let loadError     = null

// Try local dev build first, then fall back to platform npm package
const localFile = triples[key]
if (localFile) {
  const localPath = join(__dirname, localFile)
  if (existsSync(localPath)) {
    try { nativeBinding = require(localPath) }
    catch (e) { loadError = e }
  }
}

if (!nativeBinding) {
  try { nativeBinding = require(`./${name}.node`) }
  catch (e) { loadError = loadError ?? e }
}

if (!nativeBinding) {
  throw loadError ?? new Error(
    `Failed to load native addon for ${platform}-${arch}. ` +
    `Run \`yarn build\` inside packages/octocode-minifier-utils to compile it.`
  )
}

// ── Named exports (explicit — not a spread) ──────────────────────────────────
// Rule: re-export named symbols explicitly for tree-shaking and type safety.

// Constants
module.exports.SIGNATURES_ONLY_HINT = nativeBinding.SIGNATURES_ONLY_HINT

// File-extension util
module.exports.getExtension = nativeBinding.getExtension

// Core minification
module.exports.minifyContentSync   = nativeBinding.minifyContentSync
module.exports.minifyContentResult = nativeBinding.minifyContentResult
module.exports.applyMinification   = nativeBinding.applyMinification
module.exports.applyContentViewMinification = nativeBinding.applyContentViewMinification

// Async-compatible wrapper — Rust runs sync; result wrapped in Promise.resolve()
// so existing `await minifyContent(...)` call sites continue to work unchanged.
module.exports.minifyContent = function minifyContent(content, filePath) {
  try {
    return Promise.resolve(nativeBinding.minifyContentResult(content, filePath))
  } catch (e) {
    return Promise.reject(e)
  }
}

// Fine-grained strategy exports
module.exports.removeComments          = nativeBinding.removeComments
module.exports.stripPythonDocstrings   = nativeBinding.stripPythonDocstrings
module.exports.minifyConservativeCore  = nativeBinding.minifyConservativeCore
module.exports.minifyAggressiveCore    = nativeBinding.minifyAggressiveCore
module.exports.minifyJsonCore          = nativeBinding.minifyJsonCore
module.exports.minifyJsonReadable      = nativeBinding.minifyJsonReadable
module.exports.minifyCodeCore          = nativeBinding.minifyCodeCore
module.exports.minifyGeneralCore       = nativeBinding.minifyGeneralCore
module.exports.minifyMarkdownCore      = nativeBinding.minifyMarkdownCore
module.exports.minifyCSSCore           = nativeBinding.minifyCSSCore
module.exports.minifyHTMLCore          = nativeBinding.minifyHTMLCore
module.exports.minifyJavaScriptCore    = nativeBinding.minifyJavaScriptCore
module.exports.minifyCSSQuality        = nativeBinding.minifyCSSQuality
module.exports.minifyHTMLQuality       = nativeBinding.minifyHTMLQuality

// Signature extraction
module.exports.extractSignatures             = nativeBinding.extractSignatures
module.exports.getSupportedSignatureExtensions = nativeBinding.getSupportedSignatureExtensions

// YAML serialization
module.exports.jsonToYamlString = nativeBinding.jsonToYamlString
