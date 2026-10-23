// Recorded MCP calls: every call appends one trace row with its phase, trial, arm, timing and
// exact response size. A persistent arm sets `session.client`; otherwise each call connects,
// calls and closes its own client.
import { appendFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { connectStdio } from '../../dist/runtime.js';

export const session = {
  phase: 'setup',
  current: null,
  client: null,
  records: [],
  trace: null,
};

export async function connect() {
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

export async function call(name, input) {
  const begin = performance.now();
  let setupMs = 0,
    closeMs = 0,
    active = session.client;
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
  if (!session.client) {
    const t = performance.now();
    await active.close();
    closeMs = performance.now() - t;
  }
  const serialized = JSON.stringify(response ?? error);
  const row = {
    phase: session.phase,
    trial: session.current?.trial,
    arm: session.current?.arm,
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
  session.records.push(row);
  appendFileSync(session.trace, JSON.stringify(row) + '\n');
  if (error) throw new Error(error.message);
  return response;
}

export function data(response) {
  if (response.structuredContent) return response.structuredContent;
  for (const c of response.content ?? [])
    if (c.type === 'text')
      try {
        return JSON.parse(c.text);
      } catch {
        // Not JSON: try the next text block.
      }
  throw new Error('No structured result');
}

export function ok(response) {
  const d = data(response);
  assert.equal(d.ok, true, JSON.stringify(d));
  return d;
}

/** Follows every query continuation and returns all rows with the final coverage. */
export async function queryAll(input) {
  let route = { tool: 'query', query: input },
    pages = 0,
    scanned,
    matched;
  const all = [];
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

/** Navigates and extracts selector; returns the complete extraction artifact path. */
export async function extraction(connection, url, selector) {
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
