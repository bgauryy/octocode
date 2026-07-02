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

// Skills managed separately by the pi-extension — not synced here.
//   octocode          — local-only meta-skill
//   octocode-stats    — local-only dashboard skill
//   octocode-awareness — bundled as dist/awareness/ tools, not as a skill dir
const SKIPPED_SKILLS = new Set(['octocode', 'octocode-stats', 'octocode-awareness']);

// Directories that are never copied (build artefacts, VCS internals).
const SKIPPED_DIRS = new Set(['.git', 'node_modules', 'dist', 'out', 'target', '__pycache__', 'coverage']);

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

function copyDir(src, dst, dryRun) {
  if (!dryRun) fs.mkdirSync(dst, { recursive: true });
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    if (isSecretEnvFile(entry.name)) continue;         // skip .env (keep .env.example)
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

// Inject octocode-config.mjs into BOTH locations:
//   1. source skills (skills/) — so scripts run directly from the repo load the full env
//   2. pi-extension skills (packages/octocode-pi-extension/skills/) — for the built extension
console.log('Injecting octocode-config.mjs into source skills (skills/):');
injectConfig(skills, SKILLS_ROOT, dryRun);
console.log('Injecting octocode-config.mjs into pi-extension skills:');
injectConfig(skills, DEST, dryRun);

const destRel = path.relative(process.cwd(), DEST);
const label = dryRun ? '[dry-run] would sync' : 'Synced';
console.log(`${label} ${skills.length} skill(s) → ${destRel}/`);
for (const s of skills) console.log(`  ${s.name}`);
if (dryRun) console.log('  (no files written; .env excluded, octocode-config.mjs injected into source + pi-extension skills)');
