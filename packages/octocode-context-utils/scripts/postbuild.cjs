/**
 * Patches the napi-rs generated index.js AND index.d.ts after every build.
 * napi build regenerates both from the Rust #[napi] exports, wiping any
 * hand-authored additions.  This script appends them back (idempotent).
 */
'use strict'

const { copyFileSync, existsSync, readFileSync, writeFileSync } = require('fs')
const { join } = require('path')

const PATCH_MARKER = '// ── postbuild additions ──'

function patch(fileName, addition) {
  const filePath = join(__dirname, '..', fileName)
  const src = readFileSync(filePath, 'utf8')
  if (src.includes(PATCH_MARKER)) {
    console.log(`${fileName} already patched — skipping`)
    return
  }
  writeFileSync(filePath, src + addition, 'utf8')
  console.log(`${fileName} patched`)
}

patch(
  'index.js',
  `
${PATCH_MARKER}

// Async-compatible drop-in for the TS \`minifyContent(content, filePath): Promise<MinifyResult>\`.
// Rust runs synchronously; we wrap in Promise.resolve() for drop-in call-site compatibility.
module.exports.minifyContent = function minifyContent(content, filePath) {
  try { return Promise.resolve(module.exports.minifyContentResult(content, filePath)) }
  catch (e) { return Promise.reject(e) }
}

// MINIFY_CONFIG mirrors the TS package shape: { fileTypes: Record<string, { strategy, comments }> }
module.exports.MINIFY_CONFIG = module.exports.getMINIFY_CONFIG()

// SUPPORTED_SIGNATURE_EXTENSIONS — sorted, frozen array (matches TS constant)
module.exports.SUPPORTED_SIGNATURE_EXTENSIONS =
  Object.freeze(module.exports.getSupportedSignatureExtensions().sort())
`
)

patch(
  'index.d.ts',
  `
${PATCH_MARKER}

/**
 * Async drop-in: Promise.resolve() around the synchronous Rust call — it does
 * NOT move work off the event loop. Failures surface as MinifyResult.failed,
 * not rejections.
 */
export declare function minifyContent(content: string, filePath: string): Promise<MinifyResult>
export declare const MINIFY_CONFIG: {
  fileTypes: Record<string, { strategy: string; comments: string | string[] | null }>
}
export declare const SUPPORTED_SIGNATURE_EXTENSIONS: readonly string[]
`
)

const PLATFORM_PACKAGES = {
  'darwin-arm64': 'darwin-arm64',
  'darwin-x64': 'darwin-x64',
  'linux-arm64-gnu': 'linux-arm64-gnu',
  'linux-x64-gnu': 'linux-x64-gnu',
  'linux-x64-musl': 'linux-x64-musl',
  'win32-x64-msvc': 'win32-x64-msvc',
}

for (const [triple, dirName] of Object.entries(PLATFORM_PACKAGES)) {
  const binaryName = `octocode-context-utils.${triple}.node`
  const sourcePath = join(__dirname, '..', binaryName)
  const packageDir = join(__dirname, '..', 'npm', dirName)
  const destinationPath = join(packageDir, binaryName)

  if (!existsSync(sourcePath) || !existsSync(packageDir)) {
    continue
  }

  copyFileSync(sourcePath, destinationPath)
  console.log(`${binaryName} copied to npm/${dirName}`)
}
