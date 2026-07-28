#!/usr/bin/env node
import { mkdirSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { resolve, join } from 'node:path';

const argv = process.argv.slice(2);
const getArg = (flag, def = '') => { const i = argv.indexOf(flag); return i >= 0 && argv[i + 1] ? argv[i + 1] : def; };
if (argv.includes('--help') || argv.includes('-h')) {
  console.log('Usage: protocol-corpus.mjs [--out .octocode/cdp-protocol] [--domains Network,Storage,DOMStorage,Page,Runtime,Input]');
  process.exit(0);
}
const out = resolve(getArg('--out', '.octocode/cdp-protocol'));
const domains = getArg('--domains', 'Network,Storage,DOMStorage,CacheStorage,Page,Runtime,Target,Browser,Fetch,Performance,Security,Accessibility,DOM,CSS,Input').split(',').map(s => s.trim()).filter(Boolean);
mkdirSync(out, { recursive: true });
const fetchScript = resolve('skills/octocode-scraping/scripts/fetch.mjs');
const results = [];
function fetchSession(url, session) {
  const resultPath = join(out, `${session}-fetch.json`);
  const res = spawnSync(process.execPath, [fetchScript, '--provider', 'direct', '--url', url, '--mode', 'html', '--session', session, '--out', out, '--extract-links'], { encoding: 'utf8', maxBuffer: 10 * 1024 * 1024 });
  writeFileSync(resultPath, res.stdout || res.stderr || '');
  let parsed = null; try { parsed = JSON.parse(res.stdout); } catch {}
  results.push({ session, url, status: res.status, ok: parsed?.ok ?? false, sessionDir: parsed?.sessionDir ?? null, resultPath });
}
fetchSession('https://chromedevtools.github.io/devtools-protocol/', 'cdp-root');
for (const domain of domains) fetchSession(`https://chromedevtools.github.io/devtools-protocol/tot/${domain}`, `cdp-${domain}`);
writeFileSync(join(out, 'protocol-corpus-summary.json'), `${JSON.stringify({ ok: results.every(r => r.status === 0 && r.ok), out, domains, results }, null, 2)}\n`);
console.log(JSON.stringify({ ok: results.every(r => r.status === 0 && r.ok), out, domains, results }, null, 2));
process.exit(results.every(r => r.status === 0 && r.ok) ? 0 : 1);
