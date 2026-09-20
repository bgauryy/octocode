#!/usr/bin/env node
/**
 * Thin alias (chrome-devtools side): jev pre-read triage over captured artifacts
 * (cdp/body-NNN.txt, HAR-derived files, snapshots) before reading them in full.
 * Owner: octocode-scraping/scripts/corpus-triage.mjs
 */
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
if (args.includes('--help') || args.includes('-h')) {
  console.log('Usage: jev-triage-local.mjs [--scraping-skill-dir <dir>] [corpus-triage options]\n\nJudge captured bodies against a goal with jev before reading them (e.g. --session-dir <scrape session> --goal "<goal>" --files cdp/body-001.txt,cdp/body-002.txt).\nOptional dependency: octocode-scraping. Install it beside this skill or pass its folder with --scraping-skill-dir.');
  process.exit(0);
}
const options = args.flatMap((value, index) => value === '--scraping-skill-dir' ? [index] : []);
const option = options[0] ?? -1;
if (options.length > 1 || (option >= 0 && (!args[option + 1] || args[option + 1].startsWith('--')))) {
  console.error(JSON.stringify({ ok: false, code: 'INVALID_ARGUMENT', error: '--scraping-skill-dir requires a directory' }));
  process.exit(2);
}
const explicit = option >= 0 ? resolve(args[option + 1]) : null;
const forwarded = option >= 0 ? args.filter((_, i) => i !== option && i !== option + 1) : args;
const candidates = [
  ...(explicit ? [join(explicit, 'scripts', 'corpus-triage.mjs')] : []),
  ...(!explicit ? [join(here, '..', '..', 'octocode-scraping', 'scripts', 'corpus-triage.mjs')] : []),
];
const target = candidates.find((p) => existsSync(p));
if (!target) {
  console.error(JSON.stringify({
    ok: false,
    code: 'OPTIONAL_DEPENDENCY_MISSING',
    error: 'The optional octocode-scraping skill is required for jev triage. Install it beside octocode-chrome-devtools or pass --scraping-skill-dir <dir>.',
  }));
  process.exit(1);
}
const result = spawnSync(process.execPath, [target, ...forwarded], {
  cwd: process.cwd(),
  encoding: 'utf8',
  stdio: 'inherit',
});
process.exit(result.status ?? 1);
