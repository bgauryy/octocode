#!/usr/bin/env node
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, isAbsolute, join, relative, resolve } from 'node:path';

import { fileURLToPath } from 'node:url';

const argv = process.argv.slice(2);
const getArg = (flag, def = '') => { const i = argv.indexOf(flag); return i >= 0 && argv[i + 1] ? argv[i + 1] : def; };
if (argv.includes('--help') || argv.includes('-h')) {
  console.log('Usage: protocol-corpus.mjs [--scraping-skill-dir <dir>] [--out .octocode/octocode-chrome-devtools/cdp-protocol] [--domains Network,Storage,DOMStorage,Page,Runtime,Input]');
  process.exit(0);
}
const workspaceOutputBase = resolve(process.cwd(), '.octocode');
const out = resolve(getArg('--out', '.octocode/octocode-chrome-devtools/cdp-protocol'));
const outRelative = relative(workspaceOutputBase, out);
if (outRelative.startsWith('..') || isAbsolute(outRelative)) {
  console.error(`--out must stay under ${workspaceOutputBase}`);
  process.exit(2);
}
const domains = getArg('--domains', 'Network,Storage,DOMStorage,CacheStorage,Page,Runtime,Target,Browser,Fetch,Performance,Security,Accessibility,DOM,CSS,Input').split(',').map(s => s.trim()).filter(Boolean);
const options = argv.flatMap((value, index) => value === '--scraping-skill-dir' ? [index] : []);
const option = options[0] ?? -1;
if (options.length > 1 || (option >= 0 && (!argv[option + 1] || argv[option + 1].startsWith('--')))) {
  console.error(JSON.stringify({ ok: false, code: 'INVALID_ARGUMENT', error: '--scraping-skill-dir requires a directory' }));
  process.exit(2);
}
const scraping = option >= 0 ? resolve(argv[option + 1]) : resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', 'octocode-scraping');
const fetchScript = join(scraping, 'scripts', 'fetch.mjs');
if (!existsSync(fetchScript)) {
  console.error(JSON.stringify({ ok: false, code: 'OPTIONAL_DEPENDENCY_MISSING', error: 'Install octocode-scraping beside this skill or pass --scraping-skill-dir <dir>.' }));
  process.exit(1);
}
mkdirSync(out, { recursive: true });
const results = [];
function fetchSession(url, session) {
  const resultPath = join(out, `${session}-fetch.json`);
  const res = spawnSync(process.execPath, [fetchScript, '--provider', 'direct', '--url', url, '--mode', 'html', '--session', session, '--out', out, '--extract-links'], { encoding: 'utf8', timeout: 60000, maxBuffer: 10 * 1024 * 1024 });
  writeFileSync(resultPath, res.stdout || res.stderr || '');
  let parsed = null; try { parsed = JSON.parse(res.stdout); } catch {}
  results.push({ session, url, status: res.status, ok: parsed?.ok ?? false, sessionDir: parsed?.sessionDir ?? null, resultPath });
}
fetchSession('https://chromedevtools.github.io/devtools-protocol/', 'cdp-root');
for (const domain of domains) fetchSession(`https://chromedevtools.github.io/devtools-protocol/tot/${domain}`, `cdp-${domain}`);
writeFileSync(join(out, 'protocol-corpus-summary.json'), `${JSON.stringify({ ok: results.every(r => r.status === 0 && r.ok), out, domains, results }, null, 2)}\n`);
console.log(JSON.stringify({ ok: results.every(r => r.status === 0 && r.ok), out, domains, results }, null, 2));
process.exit(results.every(r => r.status === 0 && r.ok) ? 0 : 1);
