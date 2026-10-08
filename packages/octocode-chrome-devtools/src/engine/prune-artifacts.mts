#!/usr/bin/env node
import { readdirSync, statSync, rmSync, existsSync } from 'node:fs';
import { isAbsolute, join, relative, resolve } from 'node:path';
import { validateFlags } from './cli-flags.mjs';

const argv = process.argv.slice(2);
const getArg = (flag: string, def?: string) => {
  const i = argv.indexOf(flag);
  return i !== -1 && argv[i + 1] ? argv[i + 1] : def;
};
const hasFlag = flag => argv.includes(flag);

if (hasFlag('--help') || hasFlag('-h')) {
  console.error(
    '[PRUNE] Usage: node prune-artifacts.mjs [--max-age-days 3] [--max-count 50] [--dry-run] [--base <dir>]'
  );
  process.exit(0);
}

validateFlags(
  argv,
  ['--max-age-days', '--max-count', '--base'],
  ['--help', '-h', '--dry-run']
);
const MAX_AGE_DAYS = Number(getArg('--max-age-days', '3'));
const MAX_COUNT = Number(getArg('--max-count', '50'));
if (
  !Number.isFinite(MAX_AGE_DAYS) ||
  MAX_AGE_DAYS < 0 ||
  !Number.isInteger(MAX_COUNT) ||
  MAX_COUNT < 0
) {
  console.error(
    '[PRUNE] Age must be nonnegative and count a nonnegative integer'
  );
  process.exit(2);
}
const failures = [];
const DRY_RUN = hasFlag('--dry-run');
const BASE_OVERRIDE = getArg('--base', null);

const WORKSPACE_OUTPUT_BASE = resolve(process.cwd(), '.octocode');
const BASE = BASE_OVERRIDE
  ? resolve(BASE_OVERRIDE)
  : join(WORKSPACE_OUTPUT_BASE, 'tmp', 'chrome-devtools');
const baseRelative = relative(WORKSPACE_OUTPUT_BASE, BASE);
if (baseRelative.startsWith('..') || isAbsolute(baseRelative)) {
  console.error(`[PRUNE] --base must stay under ${WORKSPACE_OUTPUT_BASE}`);
  process.exit(2);
}
const TIMESTAMP_RE = /^\d{4}-\d{2}-\d{2}-\d{2}-\d{2}-\d{2}(?:-\d+)?$/;
const PORT_DIR_RE = /^port-\d+$/;

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
  const expired = dirs.filter(d => now - d.mtimeMs > maxAgeMs);
  const fresh = dirs
    .filter(d => now - d.mtimeMs <= maxAgeMs)
    .sort((a, b) => b.mtimeMs - a.mtimeMs);
  const overCap = fresh.slice(MAX_COUNT);
  const toRemove = [...expired, ...overCap];

  const removed = [];
  for (const d of toRemove) {
    try {
      if (!DRY_RUN) rmSync(d.path, { recursive: true, force: true });
      removed.push(d.path);
    } catch (error) {
      failures.push({ path: d.path, error: error.message });
    }
  }
  console.error(
    `[PRUNE] ${label}: ${dirs.length} found, ${removed.length} ${DRY_RUN ? 'would remove' : 'removed'}, ${dirs.length - removed.length} kept`
  );
  return removed;
}

const runDirs = listDirs(BASE).filter(d => TIMESTAMP_RE.test(d.name));
const removedRuns = prune(runDirs, 'run directories');

const sessionMetaBase = join(BASE, 'session-meta');
const metaDirs = listDirs(sessionMetaBase).filter(d =>
  PORT_DIR_RE.test(d.name)
);
const removedMeta = prune(metaDirs, 'session-meta directories');

console.log(
  JSON.stringify(
    {
      status: failures.length ? 'PRUNE_PARTIAL' : 'PRUNE_COMPLETE',
      failures,
      dryRun: DRY_RUN,
      maxAgeDays: MAX_AGE_DAYS,
      maxCount: MAX_COUNT,
      base: BASE,
      runDirs: {
        found: runDirs.length,
        removed: removedRuns.length,
        paths: removedRuns,
      },
      sessionMetaDirs: {
        found: metaDirs.length,
        removed: removedMeta.length,
        paths: removedMeta,
      },
    },
    null,
    2
  )
);

if (failures.length) process.exitCode = 1;
