/**
 * Called after `cargo build --target <triple>` to copy the compiled
 * binaries into the matching npm/<platform>/ directory for publishing.
 *
 * Usage: node scripts/copy-binaries.cjs <platform>
 *
 * Example:
 *   cargo build --release --bins --target aarch64-apple-darwin
 *   node scripts/copy-binaries.cjs darwin-arm64
 */
'use strict'

const { copyFileSync, chmodSync, mkdirSync } = require('fs')
const { join } = require('path')

const TARGET_MAP = {
  'darwin-arm64':    'aarch64-apple-darwin',
  'darwin-x64':      'x86_64-apple-darwin',
  'linux-arm64-gnu': 'aarch64-unknown-linux-gnu',
  'linux-x64-gnu':   'x86_64-unknown-linux-gnu',
  'linux-x64-musl':  'x86_64-unknown-linux-musl',
  'win32-x64-msvc':  'x86_64-pc-windows-msvc',
}

const platform = process.argv[2]
if (!platform || !TARGET_MAP[platform]) {
  console.error('Usage: node scripts/copy-binaries.cjs <platform>')
  console.error(`Valid platforms: ${Object.keys(TARGET_MAP).join(', ')}`)
  process.exit(1)
}

const triple = TARGET_MAP[platform]
const isWindows = platform.startsWith('win32')
const ext = isWindows ? '.exe' : ''
const root = join(__dirname, '..')
const srcDir = join(root, 'target', triple, 'release')
const destDir = join(root, 'npm', platform)

mkdirSync(destDir, { recursive: true })

for (const name of ['octocode', 'octocode-regex-worker']) {
  const src = join(srcDir, `${name}${ext}`)
  const dest = join(destDir, `${name}${ext}`)
  copyFileSync(src, dest)
  if (!isWindows) {
    // Ensure the binary is executable (cargo strips this on some hosts)
    chmodSync(dest, 0o755)
  }
  console.log(`  ✔ ${name}${ext}  →  npm/${platform}/${name}${ext}`)
}

console.log(`\nCopied binaries for ${platform} (${triple})`)
