/**
 * Pre-build step run before `napi build`.
 *
 * The hand-authored loaders (index.js ESM facade, index.cjs native loader,
 * index.d.ts types) are NOT backed up here — their canonical sources live in
 * loader/, which napi-rs never touches. `napi build` clobbers the root
 * index.js/index.d.ts with a generated CJS loader; postbuild.cjs overwrites
 * them back from loader/. This removes the old "back up the live file, skip if
 * already auto-generated" guard, which could not self-heal once a root loader
 * had been clobbered with no backup on disk.
 */
'use strict'

const { execFileSync } = require('child_process')
const { join } = require('path')

const root = join(__dirname, '..')

// Regenerate octocode-engine-core/src/security/patterns.rs from the canonical TS
// pattern list before every native build so Rust stays in lockstep with JS.
function generateSecurityPatterns() {
  execFileSync(process.execPath, [join(__dirname, 'gen-patterns.mjs')], {
    stdio: 'inherit',
    cwd: root,
  })
}

generateSecurityPatterns()
