#!/usr/bin/env node
/**
 * skills/scripts/sync.mjs
 *
 * Syncs skill source directories:
 *   skills/<name>/  →  packages/octocode-pi-extension/skills/<name>/
 *
 * The pi-extension build (scripts/build.mjs) also runs this step; having it
 * standalone lets you iterate on skills without a full extension rebuild.
 *
 * Flags:
 *   --clean     Wipe the destination directory and exit.
 *   --dry-run   Print what would be copied without touching the filesystem.
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SKILLS_ROOT = path.resolve(__dirname, '..');
const DEST = path.resolve(__dirname, '../../packages/octocode-pi-extension/skills');

// Single source of truth for env/config loading — injected as octocode-config.mjs
// into every skill's scripts/ dir so skills work standalone without npm.
// Skill scripts import it via: import(new URL('./octocode-config.mjs', import.meta.url).href)
// Uses the compiled dist output (TypeScript → esbuild → dist/index.js).
const CONFIG_SRC = path.resolve(__dirname, '../../packages/octocode-config/dist/index.js');

// @octocodeai/octocode-memory is the single source of truth for all octocode-awareness
// skill scripts. The skill/ directory in the package contains source scripts (hooks,
// install.mjs, schema.mjs, etc.) and the build injects compiled binaries on top.
//
// skills/octocode-awareness/scripts/ is 100% build-generated — not in git.
const SKILL_SCRIPTS_SRC = path.resolve(
  __dirname, '../../packages/octocode-memory/skill/scripts'
);
const AWARENESS_CLI_SRC = path.resolve(
  __dirname, '../../packages/octocode-memory/dist/bin/awareness.js'
);
const EXTRACT_FILES_SRC = path.resolve(
  __dirname, '../../packages/octocode-memory/dist/bin/extract-hook-files.js'
);
const AWARENESS_SKILL = 'octocode-awareness';

// Skills NOT synced to pi-extension (handled separately or local-only).
//   octocode          — local-only meta-skill
//   octocode-stats    — local-only dashboard skill
//   octocode-awareness — has its own injection step; not a pi-extension skill dir
const SKIPPED_SKILLS = new Set(['octocode', 'octocode-stats', 'octocode-awareness']);

// Directories that are never copied (build artefacts, VCS internals).
const SKIPPED_DIRS = new Set(['.git', 'node_modules', 'dist', 'out', 'target', '__pycache__', 'coverage']);

// Files injected by this build script — always regenerated from their source packages,
// never copied verbatim from skills/ source (would carry a stale build artifact).
const INJECTED_FILES = new Set(['octocode-config.mjs', 'awareness.mjs', 'extract-hook-files.mjs']);

/**
 * Returns true for `.env` (the secret file) but NOT `.env.example`, `.env.test`, etc.
 * Gitignored local dev files — excluded from the sync so the destination stays clean.
 */
function isSecretEnvFile(name) {
  return name === '.env' || (name.startsWith('.env.') && name !== '.env.example');
}

/** Belt-and-suspenders: assert nothing slipped through after the copy. */
function assertNoSecrets(dir) {
  const violations = [];
  function walk(cur) {
    for (const entry of fs.readdirSync(cur, { withFileTypes: true })) {
      const full = path.join(cur, entry.name);
      if (entry.isFile() && isSecretEnvFile(entry.name)) {
        violations.push(path.relative(DEST, full));
      } else if (entry.isDirectory()) {
        walk(full);
      }
    }
  }
  walk(dir);
  if (violations.length > 0) {
    throw new Error(`Secret env file(s) found in destination — this is a bug:\n  ${violations.join('\n  ')}`);
  }
}

/**
 * Copy packages/octocode-config/dist/index.js → <targetRoot>/<skill.name>/scripts/octocode-config.mjs
 * for every skill entry whose source has a scripts/ subdirectory.
 *
 * Called twice:
 *   1. Into SKILLS_ROOT (source skills) — so scripts run directly from skills/ load
 *      ~/.octocode/.env and <workspace>/.octocode/.env without needing a full build.
 *   2. Into DEST (pi-extension skills) — so the built extension also has it.
 */
function injectConfig(skillEntries, targetRoot, dryRun) {
  if (!fs.existsSync(CONFIG_SRC)) {
    throw new Error(
      `Missing config source: ${CONFIG_SRC}\n` +
      `Run: node packages/octocode-config/build.mjs`
    );
  }
  let count = 0;
  for (const entry of skillEntries) {
    const srcScripts = path.join(SKILLS_ROOT, entry.name, 'scripts');
    if (!fs.existsSync(srcScripts)) continue;
    const destScripts = path.join(targetRoot, entry.name, 'scripts');
    if (!dryRun) {
      if (!fs.existsSync(destScripts)) fs.mkdirSync(destScripts, { recursive: true });
      fs.copyFileSync(CONFIG_SRC, path.join(destScripts, 'octocode-config.mjs'));
    }
    count++;
  }
  const configRel = path.relative(process.cwd(), CONFIG_SRC);
  const targetRel = path.relative(process.cwd(), targetRoot);
  console.log(`${dryRun ? '[dry-run] would inject' : 'Injected'} octocode-config.mjs (from ${configRel}) into ${count} skill scripts/ dir(s) in ${targetRel}/`);
}

/**
 * Build and inject all octocode-awareness skill scripts from @octocodeai/octocode-memory.
 *
 * Step 1 — Copy source scripts from packages/octocode-memory/skill/scripts/ into
 *   <targetRoot>/octocode-awareness/scripts/  (hooks, install.mjs, schema.mjs, etc.)
 *
 * Step 2 — Overwrite with compiled binaries from dist/bin/:
 *   awareness.mjs          — esbuild bundle (all src/ logic, only node built-ins)
 *   extract-hook-files.mjs — stdin JSON → file path list
 *
 * Step 3 — octocode-config.mjs is injected separately by injectConfig().
 *
 * The resulting scripts/ dir is 100% build-generated — never hand-edit files there.
 */
function injectMemoryCli(targetRoot, dryRun) {
  for (const src of [SKILL_SCRIPTS_SRC, AWARENESS_CLI_SRC, EXTRACT_FILES_SRC]) {
    if (!fs.existsSync(src)) {
      throw new Error(
        `Missing source: ${src}\n` +
        `Run: yarn workspace @octocodeai/octocode-memory build`
      );
    }
  }

  const destScripts = path.join(targetRoot, AWARENESS_SKILL, 'scripts');
  if (!dryRun) fs.mkdirSync(destScripts, { recursive: true });

  // Step 1: copy all source scripts (hooks, install.mjs, schema.mjs, etc.)
  if (!dryRun) {
    copyDir(SKILL_SCRIPTS_SRC, destScripts, false);
    // Make hook scripts executable
    const hooksDir = path.join(destScripts, 'hooks');
    if (fs.existsSync(hooksDir)) {
      for (const f of fs.readdirSync(hooksDir)) {
        if (f.endsWith('.sh')) fs.chmodSync(path.join(hooksDir, f), 0o755);
      }
    }
    if (fs.existsSync(path.join(destScripts, 'prune-stale-locks.sh'))) {
      fs.chmodSync(path.join(destScripts, 'prune-stale-locks.sh'), 0o755);
    }
  }
  const srcRel = path.relative(process.cwd(), SKILL_SCRIPTS_SRC);
  console.log(`  ${dryRun ? '[dry-run] would copy' : 'Copied'} ${srcRel}/ → ${path.relative(process.cwd(), destScripts)}/`);

  // Step 2: overwrite with compiled binaries
  for (const [src, destName] of [
    [AWARENESS_CLI_SRC,   'awareness.mjs'],
    [EXTRACT_FILES_SRC,   'extract-hook-files.mjs'],
    [CONFIG_SRC,          'octocode-config.mjs'],
  ]) {
    const dest = path.join(destScripts, destName);
    if (!dryRun) fs.copyFileSync(src, dest);
    console.log(`  ${dryRun ? '[dry-run] would inject' : 'Injected'} ${destName} → ${path.relative(process.cwd(), dest)}`);
  }
}

function copyDir(src, dst, dryRun) {
  if (!dryRun) fs.mkdirSync(dst, { recursive: true });
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    if (isSecretEnvFile(entry.name)) continue;         // skip .env (keep .env.example)
    if (INJECTED_FILES.has(entry.name)) continue;      // skip injected build artifacts
    if (entry.isDirectory() && SKIPPED_DIRS.has(entry.name)) continue;
    const s = path.join(src, entry.name);
    const d = path.join(dst, entry.name);
    if (entry.isDirectory()) {
      copyDir(s, d, dryRun);
    } else if (!dryRun) {
      fs.copyFileSync(s, d);
    }
  }
}

const args = new Set(process.argv.slice(2));
const dryRun = args.has('--dry-run');
const clean = args.has('--clean');

// ── clean ────────────────────────────────────────────────────────────────────
if (clean) {
  if (!dryRun) fs.rmSync(DEST, { recursive: true, force: true });
  console.log(`${dryRun ? '[dry-run] would clean' : 'Cleaned'} ${path.relative(process.cwd(), DEST)}`);
  process.exit(0);
}

// ── collect skills to sync ───────────────────────────────────────────────────
const skills = fs
  .readdirSync(SKILLS_ROOT, { withFileTypes: true })
  .filter(e => e.isDirectory() && !SKIPPED_SKILLS.has(e.name) && e.name !== 'scripts');

if (skills.length === 0) {
  console.error('No skills found in', SKILLS_ROOT);
  process.exit(1);
}

// ── sync ─────────────────────────────────────────────────────────────────────
if (!dryRun) {
  fs.rmSync(DEST, { recursive: true, force: true });
  fs.mkdirSync(DEST, { recursive: true });
  for (const skill of skills) {
    copyDir(path.join(SKILLS_ROOT, skill.name), path.join(DEST, skill.name), false);
  }
  assertNoSecrets(DEST);   // safety net — throws only if skip logic has a bug
}

// ── inject octocode-config.mjs ───────────────────────────────────────────────
// Into both source skills and pi-extension skills so both work standalone.
console.log('Injecting octocode-config.mjs into source skills (skills/):');
injectConfig(skills, SKILLS_ROOT, dryRun);
console.log('Injecting octocode-config.mjs into pi-extension skills:');
injectConfig(skills, DEST, dryRun);

// ── inject @octocodeai/octocode-memory CLI ────────────────────────────────────
// Inject bundled awareness.mjs + extract-hook-files.mjs into octocode-awareness/scripts/
// in both source and destination so the skill is self-contained (no npm install needed).
console.log(`Injecting @octocodeai/octocode-memory binaries into ${AWARENESS_SKILL}/scripts/:`);
injectMemoryCli(SKILLS_ROOT, dryRun);
if (!dryRun && fs.existsSync(path.join(DEST, AWARENESS_SKILL))) {
  console.log(`  (also injecting into pi-extension ${AWARENESS_SKILL}/ — skipped from sync but may exist)`);
  injectMemoryCli(DEST, dryRun);
}

const destRel = path.relative(process.cwd(), DEST);
const label = dryRun ? '[dry-run] would sync' : 'Synced';
console.log(`${label} ${skills.length} skill(s) → ${destRel}/`);
for (const s of skills) console.log(`  ${s.name}`);
if (dryRun) console.log('  (no files written; .env excluded, octocode-config.mjs + awareness.mjs injected)');
