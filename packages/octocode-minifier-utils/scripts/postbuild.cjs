/**
 * Patches the napi-rs generated index.js after every build.
 * napi build regenerates index.js from the Rust #[napi] exports,
 * wiping any hand-authored additions.  This script appends them back.
 */
'use strict'

const { readFileSync, writeFileSync } = require('fs')
const { join } = require('path')

const indexPath = join(__dirname, '..', 'index.js')
let src = readFileSync(indexPath, 'utf8')

const PATCH_MARKER = '// ── postbuild additions ──'

if (src.includes(PATCH_MARKER)) {
  // Already patched (e.g. running postbuild twice)
  console.log('index.js already patched — skipping')
  process.exit(0)
}

const patch = `
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

writeFileSync(indexPath, src + patch, 'utf8')
console.log('index.js patched with minifyContent, MINIFY_CONFIG, SUPPORTED_SIGNATURE_EXTENSIONS')
