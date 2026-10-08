#!/usr/bin/env node
import { createServer } from 'node:http';
import { createServer as tcpServer } from 'node:net';
import {
  mkdirSync,
  writeFileSync,
  appendFileSync,
  readFileSync,
  readdirSync,
} from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
import { connectStdio } from '../dist/runtime.js';
function benchmarkValid(
  trials,
  expected,
  cleanupValid,
  unchanged,
  guardsValid
) {
  return (
    trials.length === expected &&
    trials.every(trial => !trial.failure) &&
    cleanupValid &&
    unchanged &&
    guardsValid
  );
}
if (process.argv.includes('--self-test')) {
  assert.equal(benchmarkValid([{ failure: null }], 1, true, true, true), true);
  for (const bad of [
    [[{ failure: { message: 'failed' } }], 1, true, true, true],
    [[], 1, true, true, true],
    [[{ failure: null }], 1, false, true, true],
    [[{ failure: null }], 1, true, false, true],
    [[{ failure: null }], 1, true, true, false],
  ])
    assert.equal(benchmarkValid(...bad), false);
  console.log(
    'Benchmark gates reject trial, cleanup, integrity, guardrail and missing-trial failures.'
  );
  process.exit(0);
}
const output = resolve(
  process.argv[2] ??
    '.octocode/benchmarks/chrome-persistent-client/results/2026-10-07'
);
mkdirSync(output, { recursive: true });
const hash = value => createHash('sha256').update(value).digest('hex');
const trace = resolve(output, 'calls.jsonl');
const runtime = readFileSync(new URL('../dist/runtime.js', import.meta.url));
const distDirectory = new URL('../dist/', import.meta.url);
const distFingerprint = () =>
  hash(
    readdirSync(distDirectory, { recursive: true, withFileTypes: true })
      .filter(entry => entry.isFile())
      .map(entry => {
        const file = resolve(entry.parentPath, entry.name);
        return [
          file.slice(new URL(distDirectory).pathname.length),
          hash(readFileSync(file)),
        ];
      })
      .sort((a, b) => a[0].localeCompare(b[0]))
      .map(row => JSON.stringify(row))
      .join('\n')
  );
const distSha256 = distFingerprint();

const manifest = {
  version: 7,
  goal: 'Equal complete evidence with persistent vs per-call MCP clients',
  primary: 'median paired total wall milliseconds',
  thresholdPercent: 20,
  guardrails:
    'same complete reconstructed rows; no failures excluded; source-pinned pagination and complete filtered coverage',
  trials: 3,
  arms: ['persistent-50', 'per-call-50', 'persistent-3', 'per-call-3'],
  data: '24 owned fixture rows, first/last boundary matches; Python public landing h1',
  isolation: 'exploratory, shared filesystem, no held-out agent scoring',
  runtimeSha256: hash(runtime),
  distSha256,
  harnessSha256: hash(readFileSync(new URL(import.meta.url))),
  tokenProxy:
    'unmeasured by runner; exact output characters and bytes retained for offline tokenizer',
  publicUrl: 'https://docs.python.org/',
};
writeFileSync(
  resolve(output, 'contract.json'),
  JSON.stringify(manifest, null, 2)
);
const server = createServer((req, res) => {
  res.setHeader('content-type', 'text/html');
  res.end(
    '<!doctype html><title>Frozen lifecycle fixture</title><h1>Research fixture</h1>' +
      Array.from(
        { length: 24 },
        (_, i) =>
          `<a href="/row/${i}">${i === 0 || i === 23 ? 'boundary' : 'evidence'} row ${i}</a>`
      ).join('')
  );
});
await new Promise(r => server.listen(0, '127.0.0.1', r));
const fixture = `http://127.0.0.1:${server.address().port}/`;
const portProbe = tcpServer();
await new Promise(r => portProbe.listen(0, '127.0.0.1', r));
const port = portProbe.address().port;
await new Promise(r => portProbe.close(r));
let phase = 'setup',
  current = null,
  client = null;
const records = [],
  trials = [];
async function connect() {
  const start = performance.now();
  const result = await connectStdio({
    command: process.execPath,
    args: [
      'packages/octocode-chrome-devtools/bin/octocode-chrome-devtools.mjs',
      '--preset',
      'research',
    ],
  });
  return { client: result, setupMs: performance.now() - start };
}
async function call(name, input) {
  const begin = performance.now();
  let setupMs = 0,
    closeMs = 0,
    active = client;
  let response, error;
  if (!active) {
    const setup = await connect();
    active = setup.client;
    setupMs = setup.setupMs;
  }
  const start = performance.now();
  try {
    response = await active.callTool({ name, arguments: input });
  } catch (e) {
    error = { name: e.name, message: e.message };
  }
  const operationMs = performance.now() - start;
  if (!client) {
    const t = performance.now();
    await active.close();
    closeMs = performance.now() - t;
  }
  const serialized = JSON.stringify(response ?? error);
  const row = {
    phase,
    trial: current?.trial,
    arm: current?.arm,
    name,
    input,
    response,
    error,
    setupMs,
    operationMs,
    closeMs,
    totalMs: performance.now() - begin,
    chars: serialized.length,
    bytes: Buffer.byteLength(serialized),
  };
  records.push(row);
  appendFileSync(trace, JSON.stringify(row) + '\n');
  if (error) throw new Error(error.message);
  return response;
}
function data(response) {
  if (response.structuredContent) return response.structuredContent;
  for (const c of response.content ?? [])
    if (c.type === 'text')
      try {
        return JSON.parse(c.text);
      } catch {}
  throw new Error('No structured result');
}
function ok(response) {
  const d = data(response);
  assert.equal(d.ok, true, JSON.stringify(d));
  return d;
}
async function queryAll(input) {
  let route = { tool: 'query', query: input },
    all = [],
    pages = 0,
    scanned,
    matched;
  while (route) {
    const d = ok(await call(route.tool, route.query));
    const page = d.data ?? d;
    assert.ok(Array.isArray(page.rows), JSON.stringify(d));
    assert.ok(
      !page.rows.some(row => row.oversized),
      'Main fixture unexpectedly oversized'
    );
    all.push(...page.rows.map(row => row.value ?? row));
    scanned = page.scanned;
    matched = page.matched;
    pages++;
    route = page.next?.tool
      ? page.next
      : page.next?.continue
        ? { tool: 'query', query: { args: page.next.continue.args.slice(1) } }
        : null;
    assert.ok(pages < 100, 'Continuation cycle');
  }
  return { rows: all, pages, scanned, matched };
}
async function extraction(connection, url, selector) {
  const d = ok(
    await call('run', {
      connection,
      plan: {
        steps: [
          { op: 'goto', url },
          { op: 'extract', selector, fields: ['text', 'href'] },
        ],
      },
    })
  );
  const route = d.next?.steps?.query;
  assert.ok(route?.file, 'Missing complete browser result continuation');
  const plan = await queryAll({ ...route, limit: 50 });
  const step = plan.rows.find(row => row.op === 'extract');
  assert.ok(step?.artifact, 'Extraction artifact unavailable');
  return step.artifact;
}
let owned = false,
  guardsValid = false;
try {
  await call('open', {
    args: ['--port', String(port), '--headless', '--url', 'about:blank'],
  });
  owned = true;
  const targets = await (
    await fetch(`http://127.0.0.1:${port}/json/list`)
  ).json();
  const target = targets.find(
    t => t.type === 'page' && t.url === 'about:blank'
  );
  assert.ok(target);
  const connection = { port, target: target.id };
  let fixtureDigest = null,
    publicDigest = null;
  for (let trial = 0; trial < 3; trial++) {
    const order = trial % 2 ? [...manifest.arms].reverse() : manifest.arms;
    for (const arm of order) {
      phase = 'workload';
      current = { trial, arm };
      const start = performance.now(),
        offset = records.length;
      let persistentSetupMs = 0;
      if (arm.startsWith('persistent')) {
        const c = await connect();
        client = c.client;
        persistentSetupMs = c.setupMs;
      }
      let failure = null,
        result;
      try {
        const limit = arm.endsWith('-3') ? 3 : 50;
        const file = await extraction(connection, fixture, 'a');
        const complete = await queryAll({
          file,
          ...(limit === 3 ? { limit } : {}),
        });
        assert.equal(complete.rows.length, 24);
        assert.equal(complete.scanned, 24);
        const digest = hash(JSON.stringify(complete.rows));
        if (fixtureDigest === null) fixtureDigest = digest;
        assert.equal(digest, fixtureDigest);
        const filtered = await queryAll({
          file,
          limit,
          where: [{ path: '/text', op: 'contains', value: 'boundary' }],
        });
        assert.equal(filtered.scanned, 24);
        assert.equal(filtered.rows.length, 2);
        assert.ok(filtered.rows[0].text.endsWith('0'));
        assert.ok(filtered.rows[1].text.endsWith('23'));
        const empty = await queryAll({
          file,
          limit,
          where: [{ path: '/text', op: 'contains', value: 'missing-sentinel' }],
        });
        assert.equal(empty.scanned, 24);
        assert.equal(empty.rows.length, 0);
        // Public workload is fixed per lifecycle/page arm; h1 retains the full selected scope.
        const publicFile = await extraction(
          connection,
          manifest.publicUrl,
          'h1'
        );
        const publicRows = await queryAll({ file: publicFile, limit });
        assert.ok(publicRows.rows.length > 0);
        const publicHash = hash(JSON.stringify(publicRows.rows));
        if (publicDigest === null) publicDigest = publicHash;
        assert.equal(
          publicHash,
          publicDigest,
          'Public selected evidence changed between arms'
        );
        result = {
          fixtureDigest: digest,
          completeRows: complete.rows.length,
          completePages: complete.pages,
          filteredRows: filtered.rows.length,
          filteredScanned: filtered.scanned,
          emptyRows: empty.rows.length,
          publicRows: publicRows.rows,
          publicDigest: publicHash,
        };
      } catch (e) {
        failure = { name: e.name, message: e.message };
        process.exitCode = 1;
      } finally {
        if (client) {
          await client.close();
          client = null;
        }
      }
      const calls = records.slice(offset);
      trials.push({
        ...current,
        persistentSetupMs,
        totalMs: performance.now() - start,
        calls: calls.length,
        operationMs: calls.reduce((s, r) => s + r.operationMs, 0),
        perCallSetupMs: calls.reduce((s, r) => s + r.setupMs, 0),
        chars: calls.reduce((s, r) => s + r.chars, 0),
        bytes: calls.reduce((s, r) => s + r.bytes, 0),
        result,
        failure,
      });
      writeFileSync(
        resolve(output, 'results.json'),
        JSON.stringify({ manifest, trials }, null, 2)
      );
      console.log(JSON.stringify(trials.at(-1)));
    }
  }
  // Stale target guard: exact missing target must fail, with no mutation retries.
  phase = 'guardrails';
  current = null;
  const stale = data(
    await call('run', {
      connection: { port, target: 'missing-owned-target' },
      plan: {
        steps: [
          {
            op: 'cdp',
            method: 'Runtime.evaluate',
            params: { expression: '1' },
          },
        ],
      },
    })
  );
  assert.equal(stale.ok, false);
  // Source mutation invalidates returned continuation. Complete oversized evidence stays available.
  const corpus = resolve(output, 'guard-corpus.json');
  writeFileSync(
    corpus,
    JSON.stringify([{ text: 'x'.repeat(20000) }, { text: 'boundary' }])
  );
  const oversize = ok(await call('query', { file: corpus, limit: 1 })).data;
  assert.equal(oversize.rows[0].oversized, true);
  let next = oversize.rows[0].next,
    oversizeContent = '';
  while (next) {
    const response = ok(await call(next.tool, next.query));
    oversizeContent += response.data.content;
    next = response.data?.next ?? null;
  }
  assert.equal(JSON.parse(oversizeContent).value.text.length, 20000);
  const oversizedRemainder = await queryAll(oversize.next.query);
  assert.equal(oversizedRemainder.rows.length, 1);
  assert.equal(oversizedRemainder.rows[0].text, 'boundary');
  writeFileSync(corpus, JSON.stringify([{ text: 'changed' }]));
  const changed = data(await call('query', oversize.next.query));
  assert.equal(changed.ok, false);
  const numericFile = resolve(output, 'numeric-corpus.json');
  writeFileSync(numericFile, '[{"n":9007199254740992},{"n":9007199254740993}]');
  const numeric = ok(
    await call('query', {
      args: [
        '--file',
        numericFile,
        '--where',
        '[{"path":"/n","op":"gte","value":9007199254740993}]',
      ],
    })
  ).data;
  assert.equal(numeric.scanned, 2);
  assert.equal(numeric.matched, 1);
  assert.equal(numeric.rows[0].sourceIndex, 1);
  writeFileSync(
    resolve(output, 'guardrails.json'),
    JSON.stringify(
      {
        exactNumeric: numeric,
        staleTarget: stale,
        oversizedFirstPage: oversize,
        changedSource: changed,
        scope:
          'cancellation/queued/shutdown additionally measured by existing invocation unit tests',
      },
      null,
      2
    )
  );
  guardsValid = true;
} catch (error) {
  writeFileSync(
    resolve(output, 'fatal.json'),
    JSON.stringify(
      { phase, error: { name: error.name, message: error.message } },
      null,
      2
    )
  );
  process.exitCode = 1;
} finally {
  if (client) {
    await client.close();
    client = null;
  }
  phase = 'cleanup';
  current = null;
  let cleanupValid = !owned;
  if (owned)
    try {
      ok(await call('cleanup', { args: ['--port', String(port)] }));
      cleanupValid = true;
    } catch (error) {
      process.exitCode = 1;
      writeFileSync(
        resolve(output, 'cleanup-error.json'),
        JSON.stringify({ message: error.message }, null, 2)
      );
    }
  await new Promise(r => server.close(r));
  const ending = hash(
    readFileSync(new URL('../dist/runtime.js', import.meta.url))
  );
  const unchanged =
    ending === manifest.runtimeSha256 && distSha256 === distFingerprint();
  const valid = benchmarkValid(
    trials,
    manifest.trials * manifest.arms.length,
    cleanupValid,
    unchanged,
    guardsValid
  );
  if (!valid) process.exitCode = 1;
  writeFileSync(
    resolve(output, 'decision.json'),
    JSON.stringify(
      {
        valid,
        cleanupValid,
        unchanged,
        guardsValid,
        completeTrials: trials.length,
        failedTrials: trials.filter(trial => trial.failure).length,
      },
      null,
      2
    )
  );
  writeFileSync(
    resolve(output, 'integrity.json'),
    JSON.stringify(
      {
        before: manifest.runtimeSha256,
        after: ending,
        unchanged: ending === manifest.runtimeSha256,
        distBefore: distSha256,
        distAfter: distFingerprint(),
        distUnchanged: distSha256 === distFingerprint(),
      },
      null,
      2
    )
  );
}
