#!/usr/bin/env node
/**
 * Thin bridge (chrome-devtools side): clasify pre-read triage over captured
 * artifacts before reading them in full. Owner: octocode-scraping/corpus-triage.
 */
import { spawnSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
if (args.includes('--help') || args.includes('-h')) {
  console.log('Usage: clasify-local.mjs [--scraping-skill-dir <dir>] [corpus-triage options]\n\nAssess captured bodies against a goal before reading them (e.g. --session-dir <scrape session> --goal "<goal>" --files cdp/body-001.txt,cdp/body-002.txt). The delegated helper preserves page-local Noul, Choice, or Score answers and uses Choice for this routing flow.\nOptional dependency: octocode-scraping. Install it beside this skill or pass its folder with --scraping-skill-dir.');
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
    error: 'The optional octocode-scraping skill is required for semantic assessment triage. Install it beside octocode-chrome-devtools or pass --scraping-skill-dir <dir>.',
  }));
  process.exit(1);
}
const result = spawnSync(process.execPath, [target, ...forwarded], {
  cwd: process.cwd(),
  encoding: 'utf8',
  stdio: 'inherit',
});
process.exit(result.status ?? 1);
