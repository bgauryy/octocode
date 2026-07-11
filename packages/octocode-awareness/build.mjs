#!/usr/bin/env node
/**
 * Build script for @octocodeai/octocode-awareness.
 * esbuild for JS output + tsc --emitDeclarationOnly for .d.ts files.
 *
 * Outputs:
 *   dist/index.js          — library entry (imported by pi-extension, etc.)
 *   dist/bin/awareness.js  — standalone CLI (called by hook scripts)
 *   dist/bin/extract-hook-files.js — hook file-path extractor
 *   dist/bin/hook-runner.js — shared hook implementation
 */

import * as esbuild from 'esbuild';
import { rm } from 'node:fs/promises';
import { cpSync, copyFileSync, existsSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
const tscBin = resolve(__dirname, '../../node_modules/.bin/tsc');

await rm('dist', { recursive: true, force: true });

const shared = {
  bundle: true,
  platform: 'node',
  format: 'esm',
  target: 'node22',
  // Mark all Node built-ins as external — never bundle them.
  external: [
    'node:sqlite', 'node:fs', 'node:os', 'node:path', 'node:crypto',
    'node:child_process', 'node:url', 'node:module',
    'sqlite', 'fs', 'os', 'path', 'crypto', 'child_process',
  ],
  // Published artifacts are already inspectable JavaScript. Source maps more
  // than double the tarball while exposing monorepo source paths, so keep them
  // out of this zero-dependency runtime package.
  sourcemap: false,
};

// Library: imported by pi-extension and other consumers.
await esbuild.build({
  ...shared,
  entryPoints: ['src/index.ts'],
  outfile: 'dist/index.js',
});

// Bin banner: shebang + silence the node:sqlite ExperimentalWarning. The
// warning pollutes stderr on every CLI/hook call (hooks surface stderr to the
// agent). Only bin entries get this — dist/index.js is a library and must not
// patch process globals.
// The node:sqlite ExperimentalWarning is emitted during (hoisted) module import,
// before any banner statement runs — but warnings dispatch on the next tick, so
// swapping the 'warning' listener here still intercepts it.
const BIN_BANNER = [
  '#!/usr/bin/env node',
  "process.removeAllListeners('warning');",
  "process.on('warning', (w) => {",
  "  if (w?.name === 'ExperimentalWarning' && String(w?.message).includes('SQLite')) return;",
  '  console.error(w?.stack ?? String(w));',
  '});',
].join('\n');

// CLI entry: called by hook scripts as `node dist/bin/awareness.js <command>`.
await esbuild.build({
  ...shared,
  entryPoints: ['bin/awareness.ts'],
  outfile: 'dist/bin/awareness.js',
  banner: { js: BIN_BANNER },
});

// Hook helper: `node dist/bin/extract-hook-files.js` reads JSON from stdin.
await esbuild.build({
  ...shared,
  entryPoints: ['bin/extract-hook-files.ts'],
  outfile: 'dist/bin/extract-hook-files.js',
  banner: { js: '#!/usr/bin/env node' },
});

// Hook runner: all lifecycle hook logic shared by thin shell wrappers.
await esbuild.build({
  ...shared,
  entryPoints: ['bin/hook-runner.ts'],
  outfile: 'dist/bin/hook-runner.js',
  banner: { js: BIN_BANNER },
});

// Public schema tooling must also work from a copied skill folder and from an
// npm install with no ancestor node_modules. Bundle Zod into this one script;
// it remains a build-time dependency and the published runtime stays zero-dep.
await esbuild.build({
  ...shared,
  entryPoints: ['scripts/schema.mjs'],
  outfile: 'dist/scripts/schema.mjs',
  minify: true,
});

// Declarations are a published package artifact, not optional diagnostics.
// Fail closed so a clean JS bundle can never hide a missing/broken `types`
// entry after dist was removed at the start of the build.
execSync(`${tscBin} --emitDeclarationOnly --outDir dist -p tsconfig.build.json`, {
  stdio: 'inherit',
  cwd: __dirname,
});

console.log('✓ @octocodeai/octocode-awareness built → dist/');

const distSkillsDest = join(__dirname, 'dist', 'skills');

// ─── Sync package-bundled skills ───────────────────────────────────────────
// The Awareness package ships both the CLI and Agent Skills. Their canonical
// sources live under the repo-root skills/ directory; this build refreshes the
// generated scripts, local agent mirror, and published dist/skills bundle.

const repoRoot    = resolve(__dirname, '../..');
// Remove the retired package-local staging tree so the old canonical path can
// never linger after a build. Published skills are copied directly to dist/.
rmSync(join(__dirname, 'skills'), { recursive: true, force: true });
// Local-only skill install surface for repo agents. This is intentionally not
// package source and is ignored by git via the repo-level `.agents` rule.
const agentSkillsRoot = join(repoRoot, '.agents', 'skills');
const skillsDestRoot = join(repoRoot, 'skills');
const distBin     = join(__dirname, 'dist', 'bin');
const mirroredPackageSkills = new Set([
  'octocode-awareness',
  'octocode-skills',
]);
const retiredPackageSkills = [
  'octocode-agent-communication',
  'octocode-reflection',
];
const bundledFromRepoRoot = [
  {
    name: 'octocode-awareness',
    src: join(repoRoot, 'skills', 'octocode-awareness'),
  },
  {
    name: 'octocode-skills',
    src: join(repoRoot, 'skills', 'octocode-skills'),
  },
];
const generatedSkillMirrorRoots = [agentSkillsRoot];

for (const skillName of retiredPackageSkills) {
  rmSync(join(skillsDestRoot, skillName), { recursive: true, force: true });
  for (const mirrorRoot of generatedSkillMirrorRoots) {
    rmSync(join(mirrorRoot, skillName), { recursive: true, force: true });
  }
}

for (const bundled of bundledFromRepoRoot) {
  if (!existsSync(join(bundled.src, 'SKILL.md'))) {
    throw new Error(`bundled skill missing SKILL.md: ${bundled.src}`);
  }
}

const packageSkills = bundledFromRepoRoot.map(({ name }) => name);

for (const skillName of packageSkills) {
  const skillSrc = bundledFromRepoRoot.find((skill) => skill.name === skillName).src;
  const packageScriptDest = join(skillSrc, 'scripts');

  // 1. Compiled scripts. The octocode-awareness skill owns all operational
  // entrypoints. Vendored skills (e.g. octocode-skills) keep their own scripts/.
  const scriptCopies = skillName === 'octocode-awareness' ? [
    [join(distBin, 'awareness.js'), 'awareness.mjs'],
    [join(__dirname, 'dist', 'scripts', 'schema.mjs'), 'schema.mjs'],
  ] : [];
  if (skillName === 'octocode-awareness') {
    scriptCopies.push(
      [join(distBin, 'extract-hook-files.js'), 'extract-hook-files.mjs'],
      [join(distBin, 'hook-runner.js'), 'hook-runner.mjs'],
    );
  }

  if (scriptCopies.length > 0) {
    mkdirSync(packageScriptDest, { recursive: true });
    for (const [src, fileName] of scriptCopies) {
      copyFileSync(src, join(packageScriptDest, fileName));
    }
  }

  if (!mirroredPackageSkills.has(skillName)) {
    for (const mirrorRoot of generatedSkillMirrorRoots) {
      rmSync(join(mirrorRoot, skillName), { recursive: true, force: true });
    }
    continue;
  }

  // 2. Wipe and rebuild generated mirrors so removed files don't linger.
  for (const mirrorRoot of generatedSkillMirrorRoots) {
    const skillDest = join(mirrorRoot, skillName);
    const scriptDest = join(skillDest, 'scripts');

    rmSync(skillDest, { recursive: true, force: true });
    mkdirSync(skillDest, { recursive: true });

    cpSync(skillSrc, skillDest, {
      recursive: true,
      filter: (src) => !src.includes('node_modules'),
    });

    if (scriptCopies.length > 0) {
      mkdirSync(scriptDest, { recursive: true });
      for (const [src, fileName] of scriptCopies) {
        copyFileSync(src, join(scriptDest, fileName));
      }
    }
  }
}

// ─── Bundle skills into dist/skills/ ───────────────────────────────────────
// Copy after generated scripts are refreshed so npm/dist resolves the same
// awareness.mjs/schema.mjs as the repo-root canonical skill source.
rmSync(distSkillsDest, { recursive: true, force: true });
mkdirSync(distSkillsDest, { recursive: true });
for (const bundled of bundledFromRepoRoot) {
  cpSync(bundled.src, join(distSkillsDest, bundled.name), {
    recursive: true,
    filter: (src) => !src.includes('node_modules'),
  });
}

console.log(`✓ package-bundled skills refreshed: ${packageSkills.join(', ')}`);
console.log(`✓ skills bundled into dist/skills/ (${readdirSync(distSkillsDest).join(', ')})`);
console.log(`✓ package-bundled skill mirrors refreshed: ${[...mirroredPackageSkills].join(', ')} → .agents/skills/`);
console.log(`✓ retired awareness skill mirrors pruned: ${retiredPackageSkills.join(', ')}`);
console.log('✓ Pi extension skill output is owned by packages/octocode-pi-extension/scripts/build.mjs');
console.log('✓ local agent skill install mirror refreshed → .agents/skills/ (ignored, not source)');
