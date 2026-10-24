// Shared fixtures for the scraping script tests.
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const scripts = fileURLToPath(new URL('../', import.meta.url));
/** Runs one scraping script synchronously. */
export const node = (script, args, opts = {}) => spawnSync(process.execPath, [join(scripts, script), ...args], { encoding: 'utf8', ...opts });
/** Runs fn in a fresh temp directory that is always removed. */
export const withDir = async (fn) => { const dir = mkdtempSync(join(tmpdir(), 'scrape-test-')); try { return await fn(dir); } finally { rmSync(dir, { recursive: true, force: true }); } };
