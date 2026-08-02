/**
 * Restores the hand-authored loaders after `napi build`.
 *
 * Runtime behavior must stay pure native/Rust: the root loaders only load the
 * compiled .node addon from the local workspace or the resolved optional
 * platform package. `napi build` overwrites the root index.js/index.d.ts with a
 * generated napi-rs CJS loader — which is wrong for a "type": "module" package
 * (its `import` condition would load CJS syntax and crash). We discard those
 * generated artifacts and copy the canonical loaders back from loader/.
 *
 * loader/ is the single source of truth and is never touched by napi-rs, so this
 * restore is unconditional and always self-heals — no backup dotfiles, no
 * "skip if already generated" guard.
 */
'use strict'

const { copyFileSync, existsSync, unlinkSync } = require('fs')
const { join } = require('path')

const root = join(__dirname, '..')
const loaderDir = join(root, 'loader')

// Canonical hand-authored loaders, restored over whatever napi-rs generated.
const LOADER_FILES = ['index.js', 'index.cjs', 'index.d.ts']

function restoreLoaders() {
  for (const fileName of LOADER_FILES) {
    const sourcePath = join(loaderDir, fileName)
    if (!existsSync(sourcePath)) {
      throw new Error(
        `Missing canonical loader source loader/${fileName}. ` +
          `It is the source of truth restored after napi build — do not delete it.`
      )
    }
    copyFileSync(sourcePath, join(root, fileName))
    console.log(`restored ${fileName} from loader/`)
  }
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
    const binaryName = `octocode-engine.${triple}.node`
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

function isDevBuild() {
  return process.env.npm_lifecycle_event === 'build:dev'
}

// Snapshot the napi-generated index.d.ts (the real ABI surface) BEFORE
// restoreLoaders() overwrites it with the hand-authored loader/index.d.ts.
// scripts/check-napi-abi.cjs diffs this snapshot's exported symbol surface
// against loader/index.d.ts so a Rust ABI change that the hand types forget to
// mirror fails the build instead of drifting silently.
function snapshotNapiAbi() {
  const generated = join(root, 'index.d.ts')
  if (!existsSync(generated)) return
  copyFileSync(generated, join(root, '.napi-abi-snapshot.d.ts'))
  console.log('snapshotted napi-generated index.d.ts -> .napi-abi-snapshot.d.ts')
}

snapshotNapiAbi()
restoreLoaders()
discardGeneratedFiles()

if (isDevBuild()) {
  // Debug/native dev builds are intentionally large; keep them at the package
  // root for local tests without poisoning the optional npm platform packages.
  console.log('platform package binary copy skipped for build:dev; root artifact retained for local tests')
} else {
  copyPlatformBinaries()
}
