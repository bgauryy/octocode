#!/usr/bin/env node
import { mkdirSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { isAbsolute, join, relative, resolve } from 'node:path';

import { resolveScrapingTool } from './scraping-bridge.mjs';
import { flagArgs, validateFlags } from './cli-flags.mjs';
import { octocodeWorkspace } from './engine-io.mjs';

const argv = process.argv.slice(2);
const { getArg } = flagArgs(argv);
if (argv.includes('--help') || argv.includes('-h')) {
  console.log(
    'Usage: protocol-corpus.mjs [--scraping-skill-dir <dir>] [--out .octocode/octocode-chrome-devtools/tmp/cdp-protocol] [--domains Network,Storage,DOMStorage,Page,Runtime,Input]'
  );
  process.exit(0);
}
validateFlags(argv, ['--scraping-skill-dir', '--out', '--domains']);
const workspaceOutputBase = octocodeWorkspace();
const out = resolve(
  getArg('--out') || '.octocode/octocode-chrome-devtools/tmp/cdp-protocol'
);
const outRelative = relative(workspaceOutputBase, out);
if (outRelative.startsWith('..') || isAbsolute(outRelative)) {
  console.error(`--out must stay under ${workspaceOutputBase}`);
  process.exit(2);
}
const domains = (
  getArg('--domains') ||
  'Network,Storage,DOMStorage,CacheStorage,Page,Runtime,Target,Browser,Fetch,Performance,Security,Accessibility,DOM,CSS,Input'
)
  .split(',')
  .map(s => s.trim())
  .filter(Boolean);
const { target: fetchScript } = resolveScrapingTool(
  'fetch',
  argv,
  'Pass --scraping-skill-dir <dir> for the optional octocode-scraping skill.'
);
mkdirSync(out, { recursive: true });
type FetchResult = {
  session: string;
  url: string;
  status: number | null;
  ok: boolean;
  sessionDir: string | null;
  resultPath: string;
};
const results: FetchResult[] = [];
function fetchSession(url: string, session: string) {
  const resultPath = join(out, `${session}-fetch.json`);
  const res = spawnSync(
    process.execPath,
    [
      fetchScript,
      '--provider',
      'direct',
      '--url',
      url,
      '--mode',
      'html',
      '--session',
      session,
      '--out',
      out,
    ],
    { encoding: 'utf8', timeout: 60000, maxBuffer: 10 * 1024 * 1024 }
  );
  writeFileSync(resultPath, res.stdout || res.stderr || '');
  let parsed: { ok?: boolean; sessionDir?: string } | null = null;
  try {
    parsed = JSON.parse(res.stdout);
  } catch {}
  results.push({
    session,
    url,
    status: res.status,
    ok: parsed?.ok ?? false,
    sessionDir: parsed?.sessionDir ?? null,
    resultPath,
  });
}
fetchSession('https://chromedevtools.github.io/devtools-protocol/', 'cdp-root');
for (const domain of domains)
  fetchSession(
    `https://chromedevtools.github.io/devtools-protocol/tot/${domain}`,
    `cdp-${domain}`
  );
writeFileSync(
  join(out, 'protocol-corpus-summary.json'),
  `${JSON.stringify({ ok: results.every(r => r.status === 0 && r.ok), out, domains, results }, null, 2)}\n`
);
console.log(
  JSON.stringify(
    { ok: results.every(r => r.status === 0 && r.ok), out, domains, results },
    null,
    2
  )
);
process.exit(results.every(r => r.status === 0 && r.ok) ? 0 : 1);
