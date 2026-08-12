#!/usr/bin/env node
/**
 * Run all hermetic scraping evals. Optional live fetch smoke: OCTOCODE_LIVE_BENCH=1.
 */
import { existsSync, readFileSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const skillRoot = resolve(fileURLToPath(new URL('.', import.meta.url)), '..');
const live = process.env.OCTOCODE_LIVE_BENCH === '1';
const outBase = join(resolve(process.cwd()), '.octocode/tmp/scrape-bench-live');
const checks = [];
const assert = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail: String(detail).slice(0, 1500) });

const HERMETIC = [
  'scripts/eval-scraping.mjs',
  'scripts/eval-bridge-local-iterate.mjs',
  'scripts/eval-unified-graph.mjs',
  'scripts/eval-website-analysis.mjs',
  'scripts/eval-smart-navigation.mjs',
  'scripts/eval-agent-helpers.mjs',
  'scripts/eval-large-crawl.mjs',
  'scripts/eval-providers.mjs',
];

function runScript(rel) {
  const path = join(skillRoot, rel);
  const res = spawnSync(process.execPath, [path], {
    cwd: resolve(process.cwd()),
    encoding: 'utf8',
    timeout: 180000,
    maxBuffer: 8 * 1024 * 1024,
  });
  assert(`hermetic: ${rel}`, res.status === 0, res.stderr || res.stdout);
}

for (const rel of HERMETIC) runScript(rel);

const LIVE_TARGETS = [
  {
    id: 'example-html',
    url: 'https://example.com/',
    session: 'bench-example-html',
    assertPaths: ['AGENT_INDEX.json', 'graph/site-graph.json', 'text/page-001.clean.part-001.md'],
  },
  {
    id: 'httpbin-crawl',
    url: 'https://httpbin.org/links/5/0',
    session: 'bench-httpbin-crawl',
    crawl: true,
    maxPages: 3,
    assertPaths: ['AGENT_INDEX.json', 'indexes/pages-001.json'],
  },
];

if (live) {
  rmSync(outBase, { recursive: true, force: true });
  const fetchScript = join(skillRoot, 'scripts/fetch.mjs');
  for (const target of LIVE_TARGETS) {
    const args = [
      fetchScript,
      '--provider', 'direct',
      '--url', target.url,
      '--mode', 'html',
      '--session', target.session,
      '--out', outBase,
      '--max-text-bytes', '50000',
      '--chunk-bytes', '8000',
      '--no-raw',
    ];
    if (target.crawl) {
      args.push('--crawl', '--same-domain', '--max-pages', String(target.maxPages));
    }
    const res = spawnSync(process.execPath, args, { cwd: resolve(process.cwd()), encoding: 'utf8', timeout: 180000 });
    assert(`live fetch ${target.id}`, res.status === 0, res.stderr?.slice(0, 800) || res.stdout?.slice(0, 400));
    const sessionDir = join(outBase, target.session);
    for (const relPath of target.assertPaths || []) {
      assert(`live ${target.id} has ${relPath}`, existsSync(join(sessionDir, relPath)), sessionDir);
    }
    if (res.status === 0 && existsSync(join(sessionDir, 'AGENT_INDEX.json'))) {
      const index = JSON.parse(readFileSync(join(sessionDir, 'AGENT_INDEX.json'), 'utf8'));
      assert(`live ${target.id} corpus ok`, index.ok !== false, JSON.stringify(index.warnings?.slice(0, 2)));
    }
  }
} else {
  assert('live_smoke', true, 'skipped (set OCTOCODE_LIVE_BENCH=1 for example.com + httpbin crawl)');
}

const failed = checks.filter((c) => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, live, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
