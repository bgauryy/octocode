/**
 * Restores hand-authored public wrappers after `napi build`.
 *
 * Runtime behavior must stay pure native/Rust: index.js only loads the compiled
 * .node addon from the local workspace or required optional platform package.
 * Generated napi-rs JS/type loader files are build artifacts and are discarded.
 */
'use strict'

const {
  copyFileSync,
  existsSync,
  readFileSync,
  unlinkSync,
  writeFileSync,
} = require('fs')
const { join } = require('path')

const root = join(__dirname, '..')

function restorePublicFile(fileName, backupName) {
  const filePath = join(root, fileName)
  const backupPath = join(root, backupName)

  if (!existsSync(backupPath)) {
    return
  }

  writeFileSync(filePath, readFileSync(backupPath, 'utf8'), 'utf8')
  unlinkSync(backupPath)
  console.log(`public ${fileName} restored`)
}

function discardGeneratedFiles() {
  const generatedFiles = [join(root, 'native.cjs'), join(root, 'native.d.ts')]

  for (const generatedFile of generatedFiles) {
    if (existsSync(generatedFile)) {
      unlinkSync(generatedFile)
      console.log(`${generatedFile} removed`)
    }
  }
}

const PLATFORM_PACKAGES = {
  'darwin-arm64': 'darwin-arm64',
  'darwin-x64': 'darwin-x64',
  'linux-arm64-gnu': 'linux-arm64-gnu',
  'linux-x64-gnu': 'linux-x64-gnu',
  'linux-x64-musl': 'linux-x64-musl',
  'win32-x64-msvc': 'win32-x64-msvc',
}

function copyPlatformBinaries() {
  for (const [triple, dirName] of Object.entries(PLATFORM_PACKAGES)) {
    const binaryName = `octocode-context-utils.${triple}.node`
    const sourcePath = join(root, binaryName)
    const packageDir = join(root, 'npm', dirName)
    const destinationPath = join(packageDir, binaryName)

    if (!existsSync(sourcePath)) {
      continue
    }

    if (!existsSync(packageDir)) {
      throw new Error(`Missing optional package dir for ${binaryName}: ${packageDir}`)
    }

    copyFileSync(sourcePath, destinationPath)
    console.log(`${binaryName} copied to npm/${dirName}; root artifact retained for local tests`)
  }
}

restorePublicFile('index.js', 'index.public.js')
restorePublicFile('index.d.ts', 'index.public.d.ts')
discardGeneratedFiles()
copyPlatformBinaries()
