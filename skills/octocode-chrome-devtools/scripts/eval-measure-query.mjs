#!/usr/bin/env node
/**
 * Hermetic grader: measure-query.mjs can filter findings / failures / keys / health.
 */
import { mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';

const root = join(tmpdir(), `octocode-measure-query-${Date.now()}`);
mkdirSync(root, { recursive: true });

writeFileSync(join(root, 'performance-measure.json'), JSON.stringify({
  fcp: 2200,
  score: {
    health: 70,
    findings: [{ code: 'SLOW_FCP', ms: 2200 }],
    slowResources: [{ name: 'https://cdn.example/app.js', duration: 1500 }],
  },
  resources: [{ name: 'https://cdn.example/app.js', duration: 1500, initiatorType: 'script' }],
  measures: [],
}));

writeFileSync(join(root, 'network-measure.json'), JSON.stringify({
  counts: { requests: 3, failed: 1, slow: 1 },
  byKind: { api: 1, other: 2 },
  byStatus: { 200: 2, 404: 1 },
  health: 80,
  findings: [{ code: 'HTTP_FAILURES', count: 1 }],
  failures: [{ status: 404, method: 'GET', url: 'https://example.test/missing', ms: 12, kind: 'other' }],
  slow: [{ status: 200, method: 'GET', url: 'https://example.test/api/slow', ms: 1800, kind: 'api' }],
  sample: [
    { status: 200, method: 'GET', url: 'https://example.test/api/ok', ms: 40, kind: 'api' },
    { status: 404, method: 'GET', url: 'https://example.test/missing', ms: 12, kind: 'other' },
    { status: 200, method: 'GET', url: 'https://example.test/api/slow', ms: 1800, kind: 'api' },
  ],
}));

writeFileSync(join(root, 'storage-measure.json'), JSON.stringify({
  cookies: {
    count: 1,
    rows: [{ name: 'session_demo', domain: 'storage.test', httpOnly: false, secure: false, sameSite: 'Lax' }],
  },
  storage: {
    localStorageKeys: ['theme', 'tracking_id'],
    sessionStorageKeys: ['step'],
    suspiciousLocalKeys: ['tracking_id'],
    suspiciousSessionKeys: [],
    indexedDBDatabases: [],
    cacheNames: [],
    serviceWorkers: [],
  },
  score: {
    health: 75,
    findings: [
      { code: 'SENSITIVE_COOKIE_NOT_HTTPONLY', count: 1, names: ['session_demo'] },
      { code: 'SUSPICIOUS_LOCALSTORAGE_KEYS', keys: ['tracking_id'] },
    ],
  },
}));

const script = 'skills/octocode-chrome-devtools/scripts/cdp-checks/measure-query.mjs';
const run = (args) => {
  const r = spawnSync(process.execPath, [script, ...args], { encoding: 'utf8' });
  if (r.status !== 0) {
    console.error(r.stderr || r.stdout);
    throw new Error(`measure-query failed: ${args.join(' ')}`);
  }
  return JSON.parse(r.stdout);
};

let failed = 0;
const assert = (cond, msg) => {
  if (!cond) {
    failed += 1;
    console.error(`[FAIL] ${msg}`);
  } else {
    console.log(`[PASS] ${msg}`);
  }
};

try {
  const summary = run(['--dir', root, '--view', 'summary']);
  assert(summary.ok === true, 'summary ok');
  assert(summary.health.network === 80, 'network health');
  assert(summary.findings.some((f) => f.code === 'HTTP_FAILURES'), 'findings include HTTP_FAILURES');

  const code = run(['--dir', root, '--view', 'findings', '--code', 'SLOW_FCP']);
  assert(code.findings.length === 1 && code.findings[0].code === 'SLOW_FCP', 'code filter SLOW_FCP');

  const api = run(['--dir', root, '--view', 'sample', '--kind', 'api']);
  assert(api.sample.length === 2 && api.sample.every((r) => r.kind === 'api'), 'kind=api sample');

  const fails = run(['--dir', root, '--view', 'failures', '--domain', 'example.test']);
  assert(fails.failures.length === 1 && fails.failures[0].status === 404, 'failures + domain');

  const keys = run(['--dir', root, '--view', 'keys', '--name-regex', 'track']);
  assert(keys.keys.suspicious.some((k) => k.key === 'tracking_id'), 'suspicious key query');

  const healthGate = run(['--dir', root, '--view', 'findings', '--max-health', '78']);
  assert(
    healthGate.findings.every((f) => f.source !== 'network'),
    'max-health excludes healthier network findings when net=80',
  );
  assert(
    healthGate.findings.some((f) => f.source === 'storage' || f.source === 'performance'),
    'max-health still includes lower-health sources',
  );

  console.log(failed === 0 ? '[METRIC] MEASURE_QUERY pass=1' : `[METRIC] MEASURE_QUERY pass=0 fails=${failed}`);
  process.exit(failed === 0 ? 0 : 1);
} finally {
  rmSync(root, { recursive: true, force: true });
}
