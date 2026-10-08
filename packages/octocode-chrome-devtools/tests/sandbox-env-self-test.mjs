#!/usr/bin/env node
// Every environment knob a ready-made check (or the template) reads must reach it through cdp-sandbox.mjs.
// The sandbox forwards only an allowlist, so a knob missing from it silently falls back to the check's default
// (for example MEASURE_URL ignored: the check measures its built-in fixture instead of the requested page).
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const scripts = join(import.meta.dirname, '../dist/engine');
if (process.argv.includes('--help') || process.argv.includes('-h')) {
  console.log(
    'Usage: sandbox-env-self-test.mjs\n\nFails when a cdp-checks script reads process.env.NAME that cdp-sandbox.mjs does not forward.'
  );
  process.exit(0);
}

const sandbox = readFileSync(join(scripts, 'cdp-sandbox.mjs'), 'utf8');
const list = sandbox.match(/const SCRIPT_ENV_ALLOWLIST = \[([\s\S]*?)\];/);
if (!list) throw new Error('SCRIPT_ENV_ALLOWLIST not found in cdp-sandbox.mjs');
const forwarded = new Set(
  [...list[1].matchAll(/["']([A-Z0-9_]+)["']/g)].map(match => match[1])
);

// Set by the sandbox itself for every child, so scripts may read them without an allowlist entry.
const provided = new Set([
  'CDP_OUTPUT_DIR',
  'CDP_SESSION_META_DIR',
  'CDP_VERBOSE',
  'TMPDIR',
  'TMP',
  'TEMP',
  'SystemRoot',
  'WINDIR',
]);

const files = [
  ...readdirSync(join(scripts, 'cdp-checks'))
    .filter(name => name.endsWith('.mjs'))
    .map(name => join(scripts, 'cdp-checks', name)),
  join(scripts, 'cdp-template.mjs'),
];
const missing = [];
for (const file of files) {
  const text = readFileSync(file, 'utf8');
  for (const match of text.matchAll(
    /process\.env(?:\.([A-Za-z_][A-Za-z0-9_]*)|\[\s*['"]([A-Za-z_][A-Za-z0-9_]*)['"]\s*\])/g
  )) {
    const name = match[1] ?? match[2];
    if (!forwarded.has(name) && !provided.has(name))
      missing.push(`${file.slice(scripts.length + 1)}: ${name}`);
  }
}
if (missing.length > 0) {
  console.error(
    `cdp-sandbox.mjs does not forward these env vars, so the checks ignore them:\n${[...new Set(missing)].map(line => `  ${line}`).join('\n')}\nAdd them to SCRIPT_ENV_ALLOWLIST.`
  );
  process.exit(1);
}
console.log(
  JSON.stringify({ ok: true, forwarded: forwarded.size, scripts: files.length })
);
