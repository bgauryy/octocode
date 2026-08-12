#!/usr/bin/env node
/**
 * Thin alias (chrome-devtools side): local regex/script over scrape session or CDP artifact dir.
 * Owner: octocode-scraping/scripts/corpus-run.mjs
 */
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const candidates = [
  join(here, '..', '..', 'octocode-scraping', 'scripts', 'corpus-run.mjs'),
  join(process.cwd(), 'skills', 'octocode-scraping', 'scripts', 'corpus-run.mjs'),
];
const target = candidates.find((p) => existsSync(p));
if (!target) {
  console.error(JSON.stringify({
    ok: false,
    error: 'octocode-scraping corpus-run.mjs not found beside this skill (expected sibling octocode-scraping/)',
  }));
  process.exit(1);
}
const result = spawnSync(process.execPath, [target, ...process.argv.slice(2)], {
  cwd: process.cwd(),
  encoding: 'utf8',
  stdio: 'inherit',
});
process.exit(result.status ?? 1);
