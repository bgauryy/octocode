import test from 'node:test';
import assert from 'node:assert/strict';
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { captureResult } from '../../dist/engine/capture-result.mjs';
import { connectStdio } from '../../dist/runtime.js';
const bin = resolve(
  import.meta.dirname,
  '../../bin/octocode-chrome-devtools.mjs'
);
async function fixture(fn) {
  const work = mkdtempSync(join(tmpdir(), 'octo-capture-'));
  let client;
  try {
    client = await connectStdio({
      command: process.execPath,
      args: [bin],
      cwd: work,
    });
    await fn(work, client);
  } finally {
    await client?.close();
    rmSync(work, { recursive: true, force: true });
  }
}
async function shape(work, text, error = '', command = 'run') {
  const directory = join(work, 'capture');
  mkdirSync(directory);
  const outputs = {};
  for (const [name, value] of Object.entries({ stdout: text, stderr: error })) {
    const file = join(directory, name + '.txt');
    writeFileSync(file, value);
    outputs[name] = { file, bytes: Buffer.byteLength(value) };
  }
  return captureResult(
    { exitCode: error ? 1 : 0 },
    outputs,
    directory,
    command,
    undefined,
    work
  );
}
const call = (client, next) =>
  client.callTool({ name: next.tool, arguments: next.query }).then(r => {
    assert(!r.isError, r.content?.[0]?.text);
    return r.structuredContent;
  });
test('capture inventory pages every artifact and preserves all findings and logs', () =>
  fixture(async (work, client) => {
    const lines = [];
    for (let i = 0; i < 60; i++) {
      const file = join(work, 'row-' + i + '.json');
      writeFileSync(file, JSON.stringify([{ text: 'needle ' + i }]));
      lines.push('[ARTIFACT] row ' + file);
    }
    lines.push(lines[0], '[FINDING] pending request remains unobserved');
    const result = await shape(work, lines.join('\n'));
    assert.equal(result.capture.artifacts, 60);
    assert.equal(result.capture.findings, 1);
    assert(!JSON.stringify(result).includes('row-59'));
    assert(Buffer.byteLength(JSON.stringify(result)) < 2500);
    let next = result.next.artifacts,
      paths = result.artifacts.map(x => resolve(result.root, x.path));
    while (next) {
      const r = await call(client, next);
      assert(r.data);
      paths.push(...r.data.rows.map(x => x.value.path));
      next = r.data.next;
    }
    assert.equal(paths.length, 60);
    assert.equal(new Set(paths).size, 60);
    let page = await call(client, result.next.capture),
      text = page.data.content;
    while (page.data.next) {
      page = await call(client, page.data.next);
      text += page.data.content;
    }
    const manifest = JSON.parse(text);
    assert.deepEqual(manifest.findings, [
      '[FINDING] pending request remains unobserved',
    ]);
    assert.equal(readFileSync(manifest.logs.stdout, 'utf8'), lines.join('\n'));
  }));
test('JSON reader pages and oversized values keep MCP/CLI routes and reject source changes', () =>
  fixture(async (work, client) => {
    const file = join(work, 'rows.json');
    writeFileSync(
      file,
      JSON.stringify([
        { id: 1, text: 'x'.repeat(30000) },
        { id: 2, text: 'second' },
      ])
    );
    const first = (
      await client.callTool({
        name: 'query',
        arguments: { args: ['--file', file, '--limit', '1'] },
      })
    ).structuredContent;
    assert(first.ok);
    assert(first.data.rows[0].oversized);
    assert.equal(first.data.rows[0].next.tool, 'artifact');
    assert.equal(first.data.next.tool, 'query');
    let next = first.data.rows[0].next,
      restored = '';
    while (next) {
      const r = await call(client, next);
      assert(r.data);
      restored += r.data.content;
      next = r.data.next;
    }
    assert.equal(JSON.parse(restored).value.text, 'x'.repeat(30000));
    const second = await call(client, first.data.next);
    assert.equal(second.data.rows[0].value.id, 2);
    assert(!second.data.next);
    const input = join(work, 'next.json');
    writeFileSync(input, JSON.stringify(first.data.next.query));
    const cli = spawnSync(
      process.execPath,
      [bin, '/cli', first.data.next.tool, '--input', input, '--json'],
      { cwd: work, encoding: 'utf8' }
    );
    assert.equal(cli.status, 0, cli.stderr);
    assert.equal(
      JSON.parse(cli.stdout).structuredContent.data.rows[0].value.id,
      2
    );
    writeFileSync(file, '[]');
    const changed = await client.callTool({
      name: first.data.next.tool,
      arguments: first.data.next.query,
    });
    assert(changed.isError);
    assert.match(changed.content[0].text, /Capture changed/);
  }));
test('native Octocode can search the hinted capture scope and fetch exact evidence', () =>
  fixture(async work => {
    const octo = resolve(
      import.meta.dirname,
      '../../../octocode/out/octocode.js'
    );
    const file = join(work, 'response.json');
    writeFileSync(
      file,
      JSON.stringify({ body: 'Needle in captured web content' }, null, 2)
    );
    const result = await shape(work, '[ARTIFACT] RESPONSE ' + file);
    const scope = {
      ...result.search,
      path: result.search.paths[0],
      matchString: 'Needle',
      regex: 'literal',
    };
    delete scope.paths;
    const found = spawnSync(
      process.execPath,
      [octo, 'localSearch', JSON.stringify({ queries: [scope] })],
      { cwd: work, encoding: 'utf8' }
    );
    assert.equal(found.status, 0, found.stderr + found.stdout);
    assert.match(found.stdout, /Needle in captured web content/);
    const read = spawnSync(
      process.execPath,
      [
        octo,
        'localFetch',
        JSON.stringify({ queries: [{ path: file, length: 20 }] }),
      ],
      { cwd: work, encoding: 'utf8' }
    );
    assert.equal(read.status, 0, read.stderr + read.stdout);
    assert.match(read.stdout, /Needle in captured web content/);
  }));

test('input acknowledgement omits event payloads but keeps exact evidence reachable', () =>
  fixture(async (work, client) => {
    const action = join(work, 'action.json');
    const flow = join(work, 'browser-result.json');
    writeFileSync(
      action,
      JSON.stringify({ events: [{ value: 'private typed value' }] })
    );
    writeFileSync(
      flow,
      JSON.stringify({
        ok: true,
        requestedSteps: 1,
        completedSteps: 1,
        steps: [
          {
            index: 1,
            op: 'act',
            status: 'complete',
            artifact: action,
            condition: { verified: true, text: 'Saved' },
          },
        ],
        eventCoverage: [],
      })
    );
    const result = await shape(
      work,
      '[ARTIFACT] ACTION ' + action + '\n[ARTIFACT] browser-result.json ' + flow
    );
    assert.equal(result.flow.steps[0].status, 'complete');
    assert.deepEqual(result.flow.steps[0].condition, {
      verified: true,
      text: 'Saved',
    });
    assert(!JSON.stringify(result).includes('private typed value'));
    assert(!result.artifacts);
    assert(Buffer.byteLength(JSON.stringify(result)) < 1800);
    const page = await call(client, result.next.steps);
    assert.equal(page.data.rows[0].value.artifact, action);
    assert.equal(
      JSON.parse(readFileSync(action, 'utf8')).events[0].value,
      'private typed value'
    );
  }));

test('direct extraction pages retain every row', () =>
  fixture(async (work, client) => {
    const source = join(work, 'extract.json');
    const flow = join(work, 'browser-result.json');
    const rows = Array.from({ length: 200 }, (_, i) => ({
      text: (i % 2 === 0 ? 'needle ' : 'source ') + i,
    }));
    writeFileSync(source, JSON.stringify(rows));
    writeFileSync(
      flow,
      JSON.stringify({
        ok: true,
        requestedSteps: 1,
        completedSteps: 1,
        steps: [
          {
            index: 1,
            op: 'extract',
            status: 'complete',
            count: rows.length,
            artifact: source,
          },
        ],
        eventCoverage: [],
      })
    );
    const result = await shape(
      work,
      '[ARTIFACT] EXTRACT ' +
        source +
        '\n[ARTIFACT] browser-result.json ' +
        flow
    );
    const first = result.flow.steps[0].data;
    assert.equal(first.rows.length, 3);
    const restored = first.rows.map(row => row.value);
    let next = first.next,
      pages = 0;
    assert.equal(next.query.limit, 50);
    while (next) {
      assert(!next.query.args);
      pages++;
      const page = await call(client, next);
      restored.push(...page.data.rows.map(row => row.value));
      next = page.data.next;
    }
    assert.deepEqual(restored, rows);
    assert.equal(pages, 4);
    const input = {
      file: source,
      where: [{ path: '/text', op: 'contains', value: 'needle' }],
    };
    const filtered = (
      await client.callTool({ name: 'query', arguments: input })
    ).structuredContent;
    assert(filtered.ok);
    assert.equal(filtered.data.matched, 100);
    assert.equal(filtered.data.returned, 50);
    const found = filtered.data.rows;
    next = filtered.data.next;
    while (next) {
      assert(!next.query.args);
      assert.deepEqual(next.query.where, input.where);
      const page = await call(client, next);
      found.push(...page.data.rows);
      next = page.data.next;
    }
    assert.deepEqual(
      found.map(row => row.sourceIndex),
      Array.from({ length: 100 }, (_, i) => i * 2)
    );
    assert.deepEqual(
      found.map(row => row.value),
      rows.filter(row => row.text.includes('needle'))
    );
    const run = spawnSync(
      process.execPath,
      [
        bin,
        '/cli',
        'query',
        '--file',
        source,
        '--where',
        JSON.stringify(input.where),
        '--json',
      ],
      { cwd: work, encoding: 'utf8' }
    );
    assert.equal(run.status, 0, run.stderr);
    const cliPage = JSON.parse(run.stdout).structuredContent.data;
    assert.equal(cliPage.returned, 50);
    assert.deepEqual(cliPage.next.query.where, input.where);
    assert(!cliPage.next.query.args);
  }));

test('compact CLI JSON retains numeric lexemes and failed oversized step evidence', () =>
  fixture(async (work, client) => {
    const source = join(work, 'numbers.json');
    writeFileSync(
      source,
      '[{"n":9007199254740993,"negative":-0,"fraction":1.234567890123456789}]'
    );
    const run = spawnSync(
      process.execPath,
      [bin, '/cli', 'query', '--file', source, '--json'],
      { cwd: work, encoding: 'utf8' }
    );
    assert.equal(run.status, 0, run.stderr);
    for (const lexeme of [
      '9007199254740993',
      '"negative":-0',
      '1.234567890123456789',
    ])
      assert(run.stdout.includes(lexeme));
    assert.equal(run.stdout.trim().split('\n').length, 1);
    const flow = join(work, 'browser-result.json');
    const error = 'error '.repeat(10000);
    writeFileSync(
      flow,
      JSON.stringify({
        ok: false,
        requestedSteps: 2,
        completedSteps: 0,
        failure: { error },
        steps: [{ index: 1, op: 'act', status: 'failed', error }],
        eventCoverage: [],
      })
    );
    const result = await shape(work, '[ARTIFACT] browser-result.json ' + flow);
    assert(Buffer.byteLength(JSON.stringify(result)) < 2500);
    const page = await call(client, result.flow.steps[0].next);
    assert(page.data.rows[0].oversized);
    let next = page.data.rows[0].next,
      text = '';
    while (next) {
      const read = await call(client, next);
      text += read.data.content;
      next = read.data.next;
    }
    assert.equal(JSON.parse(text).value.error, error);
  }));

test('a completed input without a postcondition returns only an acknowledgement and full-evidence routes', () =>
  fixture(async (work, client) => {
    const artifact = join(work, 'input.json'),
      flow = join(work, 'browser-result.json');
    writeFileSync(
      artifact,
      JSON.stringify({ events: [{ value: 'Do not echo this input' }] })
    );
    writeFileSync(
      flow,
      JSON.stringify({
        ok: true,
        requestedSteps: 1,
        completedSteps: 1,
        steps: [{ index: 1, op: 'act', status: 'complete', artifact }],
        eventCoverage: [],
      })
    );
    const result = await shape(
      work,
      '[ARTIFACT] INPUT ' +
        artifact +
        '\n[ARTIFACT] browser-result.json ' +
        flow
    );
    assert.equal(result.flow.completedSteps, 1);
    assert.deepEqual(result.flow.steps, []);
    assert(!JSON.stringify(result).includes('Do not echo this input'));
    assert(Buffer.byteLength(JSON.stringify(result)) < 1000);
    const page = await call(client, result.next.steps);
    assert.equal(page.data.rows[0].value.artifact, artifact);
  }));

test('target inventories expose executable typed continuations through CLI and MCP', () =>
  fixture(async (work, client) => {
    const file = join(work, 'targets.json');
    const rows = Array.from({ length: 35 }, (_, index) => ({
      id: String(index),
      title: 'target ' + index,
    }));
    writeFileSync(file, JSON.stringify(rows));
    const result = await shape(
      work,
      JSON.stringify({
        rows: [],
        next: {
          continue: {
            command: process.execPath,
            args: [
              resolve(
                import.meta.dirname,
                '../../dist/engine/evidence-query.mjs'
              ),
              '--file',
              file,
              '--limit',
              '7',
            ],
          },
        },
      }),
      '',
      'targets'
    );
    assert.equal(result.data.next.tool, 'query');
    const reconstructed = [];
    let next = result.data.next;
    while (next) {
      const page = (await call(client, next)).data;
      reconstructed.push(...page.rows.map(row => row.value));
      next = page.next;
    }
    assert.deepEqual(reconstructed, rows);
    const cli = spawnSync(
      process.execPath,
      [bin, '/cli', 'query', '--input', '-', '--json'],
      {
        cwd: work,
        encoding: 'utf8',
        input: JSON.stringify(result.data.next.query),
      }
    );
    assert.equal(cli.status, 0, cli.stderr);
    assert.deepEqual(
      JSON.parse(cli.stdout).structuredContent.data.rows.map(row => row.value),
      rows.slice(0, 7)
    );
  }));
