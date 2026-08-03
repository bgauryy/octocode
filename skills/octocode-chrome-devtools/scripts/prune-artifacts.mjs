#!/usr/bin/env node
import { readdirSync, statSync, rmSync, existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { getOctocodeHome } from '@octocodeai/config';

const argv    = process.argv.slice(2);
const getArg  = (flag, def) => { const i = argv.indexOf(flag); return i !== -1 && argv[i + 1] ? argv[i + 1] : def; };
const hasFlag = (flag) => argv.includes(flag);

if (hasFlag('--help') || hasFlag('-h')) {
  console.error('[PRUNE] Usage: node prune-artifacts.mjs [--max-age-days 3] [--max-count 50] [--dry-run] [--base <dir>]');
  process.exit(0);
}

const MAX_AGE_DAYS = parseFloat(getArg('--max-age-days', '3'));
const MAX_COUNT    = parseInt(getArg('--max-count', '50'), 10);
const DRY_RUN      = hasFlag('--dry-run');
const BASE_OVERRIDE = getArg('--base', null);

function defaultOutputBase() {
  const workspace = resolve(process.cwd(), '.octocode');
  return existsSync(workspace) ? workspace : getOctocodeHome();
}

const BASE = BASE_OVERRIDE ? resolve(BASE_OVERRIDE) : join(defaultOutputBase(), 'tmp', 'chrome-devtools');
const TIMESTAMP_RE = /^\d{4}-\d{2}-\d{2}-\d{2}-\d{2}-\d{2}$/;
const PORT_DIR_RE  = /^port-\d+$/;

function listDirs(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true })
    .filter(d => d.isDirectory())
    .map(d => {
      const path = join(dir, d.name);
      return { name: d.name, path, mtimeMs: statSync(path).mtimeMs };
    });
}

// Remove anything past max age, then trim survivors down to max count (newest first).
function prune(dirs, label) {
  const maxAgeMs = MAX_AGE_DAYS * 24 * 60 * 60 * 1000;
  const now = Date.now();
  const expired  = dirs.filter(d => now - d.mtimeMs > maxAgeMs);
  const fresh    = dirs.filter(d => now - d.mtimeMs <= maxAgeMs).sort((a, b) => b.mtimeMs - a.mtimeMs);
  const overCap  = fresh.slice(MAX_COUNT);
  const toRemove = [...expired, ...overCap];

  for (const d of toRemove) {
    if (!DRY_RUN) { try { rmSync(d.path, { recursive: true, force: true }); } catch {} }
  }
  console.error(`[PRUNE] ${label}: ${dirs.length} found, ${toRemove.length} ${DRY_RUN ? 'would remove' : 'removed'}, ${dirs.length - toRemove.length} kept`);
  return toRemove.map(d => d.name);
}

const runDirs = listDirs(BASE).filter(d => TIMESTAMP_RE.test(d.name));
const removedRuns = prune(runDirs, 'run directories');

const sessionMetaBase = join(BASE, 'session-meta');
const metaDirs = listDirs(sessionMetaBase).filter(d => PORT_DIR_RE.test(d.name));
const removedMeta = prune(metaDirs, 'session-meta directories');

console.log(JSON.stringify({
  status: 'PRUNE_COMPLETE',
  dryRun: DRY_RUN,
  maxAgeDays: MAX_AGE_DAYS,
  maxCount: MAX_COUNT,
  base: BASE,
  runDirs: { found: runDirs.length, removed: removedRuns.length },
  sessionMetaDirs: { found: metaDirs.length, removed: removedMeta.length },
}, null, 2));
