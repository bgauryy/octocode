#!/usr/bin/env node
/**
 * Clone pinned versions of benchmark repos into packages/octocode-benchmark/target/.
 *
 * Usage:
 *   node benchmark/repo/clone.mjs               # clone / update all repos
 *   node benchmark/repo/clone.mjs react nextjs  # specific repos by key
 *
 * After cloning, each repo's HEAD SHA is written to pins.json for reproducibility.
 * Re-running is safe — already-cloned repos are skipped unless --force is passed.
 *
 * Chromium is large (35 GB full tree).  We do a sparse shallow clone of the
 * `base/` directory only (~250 MB), which is enough to exercise C++ LSP and AST.
 */

import { execSync, spawnSync } from 'child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dir = dirname(fileURLToPath(import.meta.url));
const TARGET = join(__dir, '../../target');
const PINS_FILE = join(__dir, 'pins.json');

mkdirSync(TARGET, { recursive: true });

// ── Repo catalogue ────────────────────────────────────────────────────────────
const REPOS = {
  zustand: {
    url: 'https://github.com/pmndrs/zustand.git',
    tag: 'v5.0.5',
    lang: 'TypeScript',
    desc: 'pmndrs/zustand – lightweight TS state management',
    depth: 1,
  },
  tokio: {
    url: 'https://github.com/tokio-rs/tokio.git',
    tag: 'tokio-1.45.0',
    lang: 'Rust',
    desc: 'tokio-rs/tokio – async Rust runtime',
    depth: 1,
  },
  'spring-boot': {
    url: 'https://github.com/spring-projects/spring-boot.git',
    tag: 'v3.5.3',
    lang: 'Java',
    desc: 'spring-projects/spring-boot – Java framework',
    depth: 1,
  },
  chromium: {
    url: 'https://chromium.googlesource.com/chromium/src.git',
    tag: null,           // googlesource has no lightweight tags — use HEAD
    lang: 'C++',
    desc: 'chromium/src base/ – C++ sparse shallow clone',
    depth: 1,
    sparse: ['base/'],   // sparse-checkout paths
    filter: 'blob:none', // partial clone: blobs fetched on demand
  },
  nextjs: {
    url: 'https://github.com/vercel/next.js.git',
    tag: 'v15.3.3',
    lang: 'JavaScript/TypeScript',
    desc: 'vercel/next.js – Next.js framework',
    depth: 1,
  },
};

// ── Helpers ───────────────────────────────────────────────────────────────────

function run(cmd, opts = {}) {
  const result = spawnSync('sh', ['-c', cmd], {
    stdio: opts.silent ? 'pipe' : 'inherit',
    encoding: 'utf8',
    ...opts,
  });
  if (result.status !== 0) {
    const msg = result.stderr?.trim() || `exit ${result.status}`;
    throw new Error(`Command failed: ${cmd}\n${msg}`);
  }
  return result.stdout?.trim() ?? '';
}

function headSha(repoPath) {
  return run(`git -C "${repoPath}" rev-parse HEAD`, { silent: true });
}

function loadPins() {
  try {
    return JSON.parse(readFileSync(PINS_FILE, 'utf8'));
  } catch {
    return {};
  }
}

function savePins(pins) {
  writeFileSync(PINS_FILE, JSON.stringify(pins, null, 2) + '\n');
}

// ── Clone / update ────────────────────────────────────────────────────────────

function cloneRepo(key, spec, force) {
  const dest = join(TARGET, key);

  if (existsSync(join(dest, '.git'))) {
    if (!force) {
      console.log(`  [skip] ${key} — already cloned (use --force to reclone)`);
      return headSha(dest);
    }
    console.log(`  [reclone] ${key}`);
    run(`rm -rf "${dest}"`);
  }

  console.log(`  [clone] ${key}  (${spec.lang})  ${spec.url}`);

  if (spec.sparse) {
    // Partial clone then sparse-checkout
    run(
      `git clone --filter=${spec.filter} --no-checkout --depth ${spec.depth} "${spec.url}" "${dest}"`
    );
    run(`git -C "${dest}" sparse-checkout init --cone`);
    run(`git -C "${dest}" sparse-checkout set ${spec.sparse.join(' ')}`);
    run(`git -C "${dest}" checkout`);
  } else if (spec.tag) {
    run(
      `git clone --depth ${spec.depth} --branch "${spec.tag}" "${spec.url}" "${dest}"`
    );
  } else {
    run(`git clone --depth ${spec.depth} "${spec.url}" "${dest}"`);
  }

  return headSha(dest);
}

// ── Main ──────────────────────────────────────────────────────────────────────

const args = process.argv.slice(2);
const force = args.includes('--force');
const keys = args.filter(a => !a.startsWith('--'));
const targets = keys.length ? keys : Object.keys(REPOS);

const unknown = targets.filter(k => !REPOS[k]);
if (unknown.length) {
  console.error(`Unknown repo keys: ${unknown.join(', ')}`);
  console.error(`Available: ${Object.keys(REPOS).join(', ')}`);
  process.exit(1);
}

const pins = loadPins();
let ok = 0, failed = 0;

for (const key of targets) {
  const spec = REPOS[key];
  console.log(`\n── ${key} (${spec.lang}) ──`);
  console.log(`   ${spec.desc}`);
  try {
    const sha = cloneRepo(key, spec, force);
    pins[key] = { sha, tag: spec.tag, lang: spec.lang, url: spec.url };
    console.log(`  ✓ SHA: ${sha}`);
    ok++;
  } catch (err) {
    console.error(`  ✗ ${err.message}`);
    failed++;
  }
}

savePins(pins);
console.log(`\n── Summary: ${ok} ok, ${failed} failed ──`);
console.log(`   Pins written to ${PINS_FILE}`);
if (failed) process.exit(1);
