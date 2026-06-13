/* eslint-disable */
/**
 * ESM loader for the octocode-minifier-utils native addon.
 *
 * Uses import.meta.url for path resolution so that when this file is bundled
 * by esbuild into dist/index.js, __dirname resolves to the bundle directory
 * (not the original source directory). All require() calls use a runtime
 * variable (_require) so esbuild does not attempt to statically bundle .node files.
 *
 * Lookup order for the native binary:
 *   1. <dir>/octocode-minifier-utils.<triple>.node  — local dev / workspace
 *   2. <dir>/runtime/minifier/<triple>.node          — bundled dist (esbuild/bun)
 *   3. <dir>/../runtime/minifier/<triple>.node       — packaged CLI distribution
 *   4. npm optional package @octocodeai/octocode-minifier-utils-<triple>
 */

import { existsSync } from 'fs'
import { join, dirname } from 'path'
import { createRequire } from 'module'
import { fileURLToPath } from 'url'

const _require = createRequire(import.meta.url)
const _dir = dirname(fileURLToPath(import.meta.url))
const { platform, arch } = process

function isMusl() {
  try {
    const { glibcVersionRuntime } = process.report?.getReport()?.header ?? {}
    return !glibcVersionRuntime
  } catch {
    return true
  }
}

const name = 'octocode-minifier-utils'

const tripleMap = {
  'darwin-arm64':        `${name}.darwin-arm64.node`,
  'darwin-x64':          `${name}.darwin-x64.node`,
  'linux-arm64-gnu':     `${name}.linux-arm64-gnu.node`,
  'linux-x64-gnu':       `${name}.linux-x64-gnu.node`,
  'linux-x64-musl':      `${name}.linux-x64-musl.node`,
  'linux-arm64-musl':    `${name}.linux-arm64-musl.node`,
  'win32-x64-msvc':      `${name}.win32-x64-msvc.node`,
}

function getPlatformKey() {
  if (platform === 'linux') {
    const musl = isMusl()
    if (arch === 'x64')   return musl ? 'linux-x64-musl'   : 'linux-x64-gnu'
    if (arch === 'arm64') return musl ? 'linux-arm64-musl'  : 'linux-arm64-gnu'
  }
  if (platform === 'darwin') {
    if (arch === 'arm64') return 'darwin-arm64'
    if (arch === 'x64')   return 'darwin-x64'
  }
  if (platform === 'win32' && arch === 'x64') return 'win32-x64-msvc'
  return null
}

const key = getPlatformKey()
const localFile = key ? tripleMap[key] : null

// npm package suffix for the optional per-platform package
const npmSuffix = key ? key : null

const candidates = []

// Env override first (CI / custom installs) — honored even when the platform
// is not in tripleMap so unsupported targets can still point at a binary.
if (process.env.OCTOCODE_MINIFIER_NATIVE_PATH) {
  candidates.push(process.env.OCTOCODE_MINIFIER_NATIVE_PATH)
}

if (localFile) {
  // 1. next to this file (local dev / workspace symlink)
  candidates.push(join(_dir, localFile))
  // 2. dist/runtime/minifier/ — populated by bundle-runtime-assets.mjs when building octocode-mcp
  candidates.push(join(_dir, 'runtime', 'minifier', localFile))
  // 3. one level up (CLI distribution layout where scripts/ lives beside runtime/)
  candidates.push(join(_dir, '..', 'runtime', 'minifier', localFile))
}

let nativeBinding = null
let loadError = null

for (const candidatePath of candidates) {
  if (existsSync(candidatePath)) {
    try {
      nativeBinding = _require(candidatePath)
      break
    } catch (e) {
      loadError = e
    }
  }
}

// npm optional package fallback (e.g. @octocodeai/octocode-minifier-utils-darwin-arm64)
if (!nativeBinding && npmSuffix) {
  try {
    nativeBinding = _require(`@octocodeai/${name}-${npmSuffix}`)
  } catch (e) {
    loadError = loadError ?? e
  }
}

// Last resort: delegate to the napi-generated CJS loader, which resolves
// triples not in tripleMap above (win32-arm64/ia32, freebsd, riscv, s390x…).
// Keeps the ESM loader a strict superset of the CJS one.
if (!nativeBinding) {
  try {
    nativeBinding = _require('./index.js')
  } catch (e) {
    loadError = loadError ?? e
  }
}

if (!nativeBinding) {
  const tried = candidates.join('\n  ')
  throw loadError ?? new Error(
    `Failed to load octocode-minifier-utils native binary for ${platform}-${arch}.\n` +
    `Tried:\n  ${tried}\n` +
    `Run \`yarn build\` inside packages/octocode-minifier-utils to compile it.`
  )
}

// ── Named exports ─────────────────────────────────────────────────────────────

export const SIGNATURES_ONLY_HINT = nativeBinding.SIGNATURES_ONLY_HINT
export const getExtension = nativeBinding.getExtension
export const minifyContentSync = nativeBinding.minifyContentSync
export const minifyContentResult = nativeBinding.minifyContentResult

/**
 * Async drop-in: Rust runs sync, wrapped in Promise.resolve().
 * Callers that `await minifyContent(...)` get a MinifyResult.
 */
export function minifyContent(content, filePath) {
  try { return Promise.resolve(nativeBinding.minifyContentResult(content, filePath)) }
  catch (e) { return Promise.reject(e) }
}

export const applyMinification = nativeBinding.applyMinification
export const applyContentViewMinification = nativeBinding.applyContentViewMinification
export const removeComments = nativeBinding.removeComments
export const minifyConservativeCore = nativeBinding.minifyConservativeCore
export const minifyAggressiveCore = nativeBinding.minifyAggressiveCore
export const minifyJsonCore = nativeBinding.minifyJsonCore
export const minifyJsonReadable = nativeBinding.minifyJsonReadable
export const minifyCodeCore = nativeBinding.minifyCodeCore
export const minifyGeneralCore = nativeBinding.minifyGeneralCore
export const minifyMarkdownCore = nativeBinding.minifyMarkdownCore
export const minifyCSSCore = nativeBinding.minifyCSSCore
export const minifyHTMLCore = nativeBinding.minifyHTMLCore
export const minifyJavaScriptCore = nativeBinding.minifyJavaScriptCore
export const minifyCSSQuality = nativeBinding.minifyCSSQuality
export const minifyHTMLQuality = nativeBinding.minifyHTMLQuality
export const stripPythonDocstrings = nativeBinding.stripPythonDocstrings
export const extractSignatures = nativeBinding.extractSignatures
export const getSupportedSignatureExtensions = nativeBinding.getSupportedSignatureExtensions
export const jsonToYamlString = nativeBinding.jsonToYamlString
export const getMINIFY_CONFIG = nativeBinding.getMINIFY_CONFIG

export const MINIFY_CONFIG = getMINIFY_CONFIG()
export const SUPPORTED_SIGNATURE_EXTENSIONS = Object.freeze(getSupportedSignatureExtensions().sort())
