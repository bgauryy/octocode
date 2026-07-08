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
  sourcemap: true,
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

// Generate TypeScript declarations.
execSync(`${tscBin} --emitDeclarationOnly --outDir dist -p tsconfig.build.json`, {
  stdio: 'inherit',
  cwd: __dirname,
});

console.log('✓ @octocodeai/octocode-awareness built → dist/');

const distSkillsDest = join(__dirname, 'dist', 'skills');

// ─── Sync package-owned skills ─────────────────────────────────────────────
// The complete awareness skill is owned by this package. The package skill
// folder (packages/octocode-awareness/skills/octocode-awareness) is the ONLY
// canonical source. They are intentionally never mirrored into the repo-root
// skills/ directory — that folder is reserved for user-facing repo skills,
// not this package's internal/compatibility skills. The only generated
// mirror is the local, gitignored agent install surface below.

const repoRoot    = resolve(__dirname, '../..');
// Local-only skill install surface for repo agents. This is intentionally not
// package source and is ignored by git via the repo-level `.agents` rule.
const agentSkillsRoot = join(repoRoot, '.agents', 'skills');
// Stale copies from before internal skills stopped being mirrored to repo-root
// skills/. Actively pruned on every build so they can't linger or drift.
const skillsDestRoot = join(repoRoot, 'skills');
const skillsSrcRoot  = join(__dirname, 'skills');
const distBin     = join(__dirname, 'dist', 'bin');
const mirroredPackageSkills = new Set([
  'octocode-awareness',
  'octocode-skills',
]);
const retiredPackageSkills = [
  'octocode-agent-communication',
  'octocode-reflection',
];
// Skills owned at repo-root skills/ and vendored into this package at build time.
// Gitignored under packages/octocode-awareness/skills/ so GitHub stays clean while
// npm `files: ["skills/**"]` still ships the built copy.
const bundledFromRepoRoot = [
  {
    name: 'octocode-skills',
    src: join(repoRoot, 'skills', 'octocode-skills'),
  },
];
const generatedSkillMirrorRoots = [agentSkillsRoot];

for (const skillName of retiredPackageSkills) {
  rmSync(join(skillsSrcRoot, skillName), { recursive: true, force: true });
  rmSync(join(skillsDestRoot, skillName), { recursive: true, force: true });
  for (const mirrorRoot of generatedSkillMirrorRoots) {
    rmSync(join(mirrorRoot, skillName), { recursive: true, force: true });
  }
}

for (const bundled of bundledFromRepoRoot) {
  const dest = join(skillsSrcRoot, bundled.name);
  if (!existsSync(join(bundled.src, 'SKILL.md'))) {
    if (existsSync(join(dest, 'SKILL.md'))) {
      console.warn(`⚠ bundled ${bundled.name} source missing at ${bundled.src}; keeping existing package copy`);
      continue;
    }
    throw new Error(`bundled skill missing SKILL.md: ${bundled.src}`);
  }
  rmSync(dest, { recursive: true, force: true });
  mkdirSync(skillsSrcRoot, { recursive: true });
  cpSync(bundled.src, dest, {
    recursive: true,
    filter: (src) => !src.includes('node_modules'),
  });
  console.log(`✓ bundled ${bundled.name} ← ${bundled.src}`);
}

const packageSkills = [];
for (const entry of readdirSync(skillsSrcRoot, { withFileTypes: true })) {
  if (!entry.isDirectory()) continue;
  const skillSrc = join(skillsSrcRoot, entry.name);
  if (!existsSync(join(skillSrc, 'SKILL.md'))) continue;
  packageSkills.push(entry.name);
}

for (const skillName of packageSkills) {
  const skillSrc = join(skillsSrcRoot, skillName);
  const packageScriptDest = join(skillSrc, 'scripts');

  // 1. Compiled scripts. The octocode-awareness skill owns all operational
  // entrypoints. Vendored skills (e.g. octocode-skills) keep their own scripts/.
  const scriptCopies = skillName === 'octocode-awareness' ? [
    [join(distBin, 'awareness.js'), 'awareness.mjs'],
    [join(__dirname, 'scripts', 'schema.mjs'), 'schema.mjs'],
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

  // Prune stale package-owned mirrors from repo-root skills/. Never delete
  // skills that are sourced FROM repo-root (bundledFromRepoRoot) — those are
  // the canonical upstream copies this package vendors at build time.
  const isVendoredFromRepoRoot = bundledFromRepoRoot.some((b) => b.name === skillName);
  if (!isVendoredFromRepoRoot) {
    rmSync(join(skillsDestRoot, skillName), { recursive: true, force: true });
  }

  if (!mirroredPackageSkills.has(skillName)) {
    // The package source is canonical. Keep generated mirror roots free of
    // package-only skills so agents do not edit generated/stale copies.
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
// Copy after generated scripts are refreshed so the npm/dist CLI resolves the
// same awareness.mjs/schema.mjs as the package-owned skill source.
rmSync(distSkillsDest, { recursive: true, force: true });
cpSync(join(__dirname, 'skills'), distSkillsDest, {
  recursive: true,
  filter: (src) => !src.includes('node_modules'),
});

console.log(`✓ package-owned skills refreshed: ${packageSkills.join(', ')}`);
console.log(`✓ skills bundled into dist/skills/ (${readdirSync(distSkillsDest).join(', ')})`);
console.log(`✓ package-owned skill mirrors refreshed: ${[...mirroredPackageSkills].join(', ')} → .agents/skills/ (repo-root skills/ is never used as a mirror target)`);
console.log(`✓ retired awareness skill mirrors pruned: ${retiredPackageSkills.join(', ')}`);
console.log('✓ Pi extension skill output is owned by packages/octocode-pi-extension/scripts/build.mjs');
console.log('✓ local agent skill install mirror refreshed → .agents/skills/ (ignored, not source)');
