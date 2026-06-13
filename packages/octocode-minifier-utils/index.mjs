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

const MAX_CONTENT_SIZE = 1024 * 1024
const SIGNATURE_EXTENSIONS = Object.freeze([
  'c', 'cc', 'cpp', 'cs', 'css', 'cxx', 'erl', 'ex', 'exs', 'go', 'h',
  'hpp', 'hrl', 'hs', 'html', 'htm', 'java', 'js', 'jsx', 'kt', 'kotlin',
  'less', 'lhs', 'lua', 'md', 'markdown', 'php', 'py', 'rb', 'rs', 'rust',
  'scala', 'scss', 'sql', 'svelte', 'swift', 'ts', 'tsx', 'vue',
].sort())

const FALLBACK_MINIFY_CONFIG = Object.freeze({
  fileTypes: {
    js: { strategy: 'conservative', comments: 'c-style' },
    jsx: { strategy: 'conservative', comments: 'c-style' },
    mjs: { strategy: 'conservative', comments: 'c-style' },
    cjs: { strategy: 'conservative', comments: 'c-style' },
    ts: { strategy: 'conservative', comments: 'c-style' },
    tsx: { strategy: 'conservative', comments: 'c-style' },
    py: { strategy: 'conservative', comments: ['hash', 'python-docstring'] },
    rs: { strategy: 'conservative', comments: 'c-style' },
    go: { strategy: 'conservative', comments: 'c-style' },
    java: { strategy: 'conservative', comments: 'c-style' },
    css: { strategy: 'aggressive', comments: 'c-style' },
    scss: { strategy: 'aggressive', comments: 'c-style' },
    less: { strategy: 'aggressive', comments: 'c-style' },
    html: { strategy: 'aggressive', comments: 'html' },
    htm: { strategy: 'aggressive', comments: 'html' },
    xml: { strategy: 'aggressive', comments: 'html' },
    svg: { strategy: 'aggressive', comments: 'html' },
    json: { strategy: 'json', comments: null },
    jsonc: { strategy: 'json', comments: 'c-style' },
    json5: { strategy: 'json', comments: 'c-style' },
    md: { strategy: 'markdown', comments: 'html' },
    markdown: { strategy: 'markdown', comments: 'html' },
    yaml: { strategy: 'conservative', comments: 'hash' },
    yml: { strategy: 'conservative', comments: 'hash' },
    sh: { strategy: 'conservative', comments: 'hash' },
    bash: { strategy: 'conservative', comments: 'hash' },
    zsh: { strategy: 'conservative', comments: 'hash' },
    sql: { strategy: 'conservative', comments: 'sql' },
  },
})

function isForcedJsFallback() {
  return process.env.OCTOCODE_MINIFIER_FORCE_JS === '1'
}

function isNativeRequired() {
  return process.env.OCTOCODE_MINIFIER_REQUIRE_NATIVE === '1'
}

function normalizeCommentGroups(commentTypes) {
  if (!commentTypes) return []
  if (typeof commentTypes === 'string') return [commentTypes]
  if (Array.isArray(commentTypes)) return commentTypes.filter(Boolean)
  return []
}

function fallbackGetExtension(filePath, options = null) {
  const fallback = options?.fallback ?? ''
  const base = String(filePath ?? '').split(/[\\/]/).pop() ?? ''
  if (!base || base === '.' || base === '..') return fallback
  const index = base.lastIndexOf('.')
  const ext = index > 0 && index < base.length - 1 ? base.slice(index + 1) : fallback
  return options?.lowercase ? ext.toLowerCase() : ext
}

function fallbackTrimBlankRuns(content) {
  return String(content)
    .split('\n')
    .map(line => line.trimEnd())
    .join('\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

function fallbackRemoveComments(content, commentTypes) {
  let output = String(content)
  const groups = normalizeCommentGroups(commentTypes)
  if (groups.some(group => ['c-style', 'template'].includes(group))) {
    output = output.replace(/\/\*[\s\S]*?\*\//g, '')
    output = output
      .split('\n')
      .map(line => line.replace(/(^|\s)\/\/.*$/, '$1').trimEnd())
      .join('\n')
  }
  if (groups.includes('html')) {
    output = output.replace(/<!--[\s\S]*?-->/g, '')
  }
  if (groups.includes('hash')) {
    output = output
      .split('\n')
      .filter(line => {
        const trimmed = line.trimStart()
        return !trimmed.startsWith('#') || trimmed.startsWith('#!')
      })
      .join('\n')
  }
  if (groups.includes('python-docstring')) {
    output = output.replace(/(^|[\n\r])\s*("""|''')[\s\S]*?\2/g, '$1')
  }
  if (groups.includes('sql')) {
    output = output
      .split('\n')
      .map(line => line.replace(/\s--.*$/, '').trimEnd())
      .join('\n')
  }
  return output
}

function fallbackMinifyGeneralCore(content) {
  return fallbackTrimBlankRuns(content)
}

function fallbackMinifyCodeCore(content) {
  return fallbackTrimBlankRuns(content)
}

function fallbackMinifyMarkdownCore(content) {
  return fallbackTrimBlankRuns(String(content).replace(/<!--[\s\S]*?-->/g, ''))
}

function fallbackMinifyJsonReadable(content) {
  try {
    return {
      content: JSON.stringify(JSON.parse(String(content)), null, 2),
      failed: false,
      type: 'json',
      strategy: 'json',
    }
  } catch {
    return {
      content: fallbackMinifyGeneralCore(content),
      failed: true,
      type: 'json',
      strategy: 'json',
      reason: 'Invalid JSON',
    }
  }
}

function fallbackMinifyJsonCore(content) {
  try {
    return {
      content: JSON.stringify(JSON.parse(String(content))),
      failed: false,
      type: 'json',
      strategy: 'json',
    }
  } catch {
    return fallbackMinifyJsonReadable(content)
  }
}

function fallbackApplyContentViewMinification(content, filePath) {
  const text = String(content)
  if (text.length > MAX_CONTENT_SIZE) return text
  const ext = fallbackGetExtension(filePath, { lowercase: true, fallback: 'txt' })
  const cfg = FALLBACK_MINIFY_CONFIG.fileTypes[ext]

  if (['json', 'jsonc', 'json5'].includes(ext)) {
    const result = fallbackMinifyJsonReadable(text)
    return result.content.length < text.length ? result.content : text
  }
  if (cfg?.strategy === 'markdown') return fallbackMinifyMarkdownCore(text)

  const stripped = fallbackRemoveComments(text, cfg?.comments)
  const result =
    cfg?.strategy === 'aggressive'
      ? stripped.replace(/\s+/g, ' ').trim()
      : fallbackMinifyCodeCore(stripped)
  return result.length < text.length ? result : text
}

function fallbackMinifyContentResult(content, filePath) {
  const text = String(content)
  if (text.length > MAX_CONTENT_SIZE) {
    return {
      content: text,
      failed: true,
      type: 'failed',
      strategy: 'failed',
      reason: `File too large: ${(text.length / 1048576).toFixed(2)}MB exceeds 1MB limit`,
    }
  }
  const ext = fallbackGetExtension(filePath, { lowercase: true, fallback: 'txt' })
  if (['json', 'jsonc', 'json5'].includes(ext)) return fallbackMinifyJsonCore(text)
  const contentView = fallbackApplyContentViewMinification(text, filePath)
  const strategy = FALLBACK_MINIFY_CONFIG.fileTypes[ext]?.strategy ?? 'general'
  return { content: contentView, failed: false, type: strategy, strategy }
}

function fallbackApplyMinification(content, filePath) {
  const text = String(content)
  const minified = fallbackMinifyContentResult(text, filePath).content
  return minified.length < text.length ? minified : text
}

function fallbackExtractSignatures(content, filePath) {
  const text = String(content)
  if (text.length > MAX_CONTENT_SIZE) return null
  const ext = fallbackGetExtension(filePath, { lowercase: true, fallback: String(filePath ?? '').toLowerCase() })
  if (!SIGNATURE_EXTENSIONS.includes(ext)) return null

  const lines = text.split('\n')
  const picked = []
  const pattern = ext === 'md' || ext === 'markdown'
    ? /^\s{0,3}#{1,6}\s+\S/
    : /^\s*(import|export|(?:export\s+)?(?:async\s+)?function\s+\w+|(?:export\s+)?class\s+\w+|(?:export\s+)?interface\s+\w+|(?:export\s+)?type\s+\w+|(?:export\s+)?enum\s+\w+|(?:export\s+)?const\s+\w+\s*=|def\s+\w+|class\s+\w+)/

  for (let i = 0; i < lines.length; i++) {
    if (pattern.test(lines[i])) {
      picked.push(`${String(i + 1).padStart(3, '0')}| ${lines[i].trimEnd()}`)
    }
  }
  return picked.length > 0 ? picked.join('\n') : null
}

function yamlScalar(value, indent = 0) {
  if (value === null) return 'null'
  if (typeof value === 'number' || typeof value === 'boolean') return String(value)
  const text = String(value)
  if (text.includes('\n')) {
    const pad = ' '.repeat(indent)
    return `${pad}|-\n${text.split('\n').map(line => `${pad}  ${line}`).join('\n')}`
  }
  if (
    text === '' ||
    /^(true|false|null|yes|no|on|off|[-?:,[\]{}#&*!|>'"%@`]|[0-9]+(?:\.[0-9]+)?)$/i.test(text) ||
    text.includes(': ')
  ) {
    return `'${text.replace(/'/g, "''")}'`
  }
  return text
}

function orderedKeys(record, config) {
  let keys = Object.keys(record)
  const priority = config?.keysPriority ?? config?.keys_priority ?? []
  const sortKeys = config?.sortKeys ?? config?.sort_keys ?? false
  if (!sortKeys && priority.length === 0) return keys
  return keys.sort((a, b) => {
    const ai = priority.indexOf(a)
    const bi = priority.indexOf(b)
    if (ai !== -1 || bi !== -1) {
      if (ai === -1) return 1
      if (bi === -1) return -1
      return ai - bi
    }
    return sortKeys ? a.localeCompare(b) : 0
  })
}

function yamlBlock(value, indent = 0, config = null) {
  const pad = ' '.repeat(indent)
  if (Array.isArray(value)) {
    if (value.length === 0) return '[]'
    return value.map(item => {
      const rendered = yamlBlock(item, indent + 2, config)
      if (item && typeof item === 'object' && !Array.isArray(item)) {
        return `${pad}-\n${rendered}`
      }
      if (rendered.includes('\n')) return `${pad}-\n${rendered}`
      return `${pad}- ${rendered}`
    }).join('\n')
  }
  if (value && typeof value === 'object') {
    const keys = orderedKeys(value, config)
    if (keys.length === 0) return '{}'
    return keys.map(key => {
      const child = value[key]
      const rendered = yamlBlock(child, indent + 2, config)
      if (
        Array.isArray(child) ||
        (child && typeof child === 'object') ||
        rendered.includes('\n')
      ) {
        return `${pad}${key}:\n${rendered}`
      }
      return `${pad}${key}: ${rendered}`
    }).join('\n')
  }
  return yamlScalar(value, indent)
}

function fallbackJsonToYamlString(jsonObject, config = null) {
  return `${yamlBlock(jsonObject, 0, config)}\n`
}

function createJsFallbackNative() {
  return {
    SIGNATURES_ONLY_HINT:
      'Native signature extraction unavailable; using a conservative JS outline when possible.',
    getExtension: fallbackGetExtension,
    minifyContentSync: (content, filePath) => fallbackMinifyContentResult(content, filePath).content,
    minifyContentResult: fallbackMinifyContentResult,
    applyMinification: fallbackApplyMinification,
    applyContentViewMinification: fallbackApplyContentViewMinification,
    removeComments: fallbackRemoveComments,
    minifyConservativeCore: (content, config = {}) =>
      fallbackMinifyCodeCore(fallbackRemoveComments(content, config.comments)),
    minifyAggressiveCore: (content, config = {}) =>
      fallbackRemoveComments(content, config.comments).replace(/\s+/g, ' ').trim(),
    minifyJsonCore: fallbackMinifyJsonCore,
    minifyJsonReadable: fallbackMinifyJsonReadable,
    minifyCodeCore: fallbackMinifyCodeCore,
    minifyGeneralCore: fallbackMinifyGeneralCore,
    minifyMarkdownCore: fallbackMinifyMarkdownCore,
    minifyCSSCore: content => fallbackRemoveComments(content, 'c-style').replace(/\s+/g, ' ').trim(),
    minifyHTMLCore: content => fallbackRemoveComments(content, 'html').replace(/\s+/g, ' ').trim(),
    minifyJavaScriptCore: content => fallbackApplyContentViewMinification(content, 'file.js'),
    minifyCSSQuality: content => fallbackRemoveComments(content, 'c-style').replace(/\s+/g, ' ').trim(),
    minifyHTMLQuality: content => fallbackRemoveComments(content, 'html').replace(/\s+/g, ' ').trim(),
    stripPythonDocstrings: content => fallbackRemoveComments(content, 'python-docstring'),
    extractSignatures: fallbackExtractSignatures,
    getSupportedSignatureExtensions: () => [...SIGNATURE_EXTENSIONS],
    jsonToYamlString: fallbackJsonToYamlString,
    getMINIFY_CONFIG: () => FALLBACK_MINIFY_CONFIG,
  }
}

let nativeBinding = null
let loadError = null

if (isForcedJsFallback()) {
  nativeBinding = createJsFallbackNative()
} else {
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
}

if (!nativeBinding) {
  if (!isNativeRequired()) {
    nativeBinding = createJsFallbackNative()
  } else {
    const tried = candidates.join('\n  ')
    throw loadError ?? new Error(
      `Failed to load octocode-minifier-utils native binary for ${platform}-${arch}.\n` +
      `Tried:\n  ${tried}\n` +
      `Run \`yarn build\` inside packages/octocode-minifier-utils to compile it.`
    )
  }
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
