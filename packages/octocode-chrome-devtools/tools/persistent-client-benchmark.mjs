#!/usr/bin/env node
// Persistent vs per-call MCP client benchmark over equal complete evidence. Modules under
// persistent-client-benchmark/: integrity (fingerprints), session (recorded calls and
// continuations), workload (one arm's reads) and guardrails (failure-path checks).
import { createServer } from 'node:http';
import { createServer as tcpServer } from 'node:net';
import { mkdirSync, writeFileSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import assert from 'node:assert/strict';
import {
  hash,
  distFingerprint,
  harnessFingerprint,
  runtimeFile,
} from './persistent-client-benchmark/integrity.mjs';
import {
  session,
  connect,
  call,
  ok,
} from './persistent-client-benchmark/session.mjs';
import { measureArm } from './persistent-client-benchmark/workload.mjs';
import { runGuardrails } from './persistent-client-benchmark/guardrails.mjs';
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
session.trace = resolve(output, 'calls.jsonl');
const runtime = readFileSync(runtimeFile);
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
  harnessSha256: harnessFingerprint(new URL(import.meta.url)),
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
const trials = [];
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
  for (let trial = 0; trial < 3; trial++) {
    const order = trial % 2 ? [...manifest.arms].reverse() : manifest.arms;
    for (const arm of order) {
      session.phase = 'workload';
      session.current = { trial, arm };
      const start = performance.now(),
        offset = session.records.length;
      let persistentSetupMs = 0;
      if (arm.startsWith('persistent')) {
        const c = await connect();
        session.client = c.client;
        persistentSetupMs = c.setupMs;
      }
      let failure = null,
        result;
      try {
        result = await measureArm({
          connection,
          fixture,
          publicUrl: manifest.publicUrl,
          limit: arm.endsWith('-3') ? 3 : 50,
        });
      } catch (e) {
        failure = { name: e.name, message: e.message };
        process.exitCode = 1;
      } finally {
        if (session.client) {
          await session.client.close();
          session.client = null;
        }
      }
      const calls = session.records.slice(offset);
      trials.push({
        ...session.current,
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
  await runGuardrails(output, port);
  guardsValid = true;
} catch (error) {
  writeFileSync(
    resolve(output, 'fatal.json'),
    JSON.stringify(
      {
        phase: session.phase,
        error: { name: error.name, message: error.message },
      },
      null,
      2
    )
  );
  process.exitCode = 1;
} finally {
  if (session.client) {
    await session.client.close();
    session.client = null;
  }
  session.phase = 'cleanup';
  session.current = null;
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
  const ending = hash(readFileSync(runtimeFile));
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
