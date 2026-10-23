import test from 'node:test';
import assert from 'node:assert/strict';
import { writeFileSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fixture, shape, call, typedCli } from './capture-fixture.mjs';
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
    const cli = typedCli(work, [
      first.data.next.tool,
      '--input',
      input,
      '--json',
    ]);
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
    const run = typedCli(work, [
      'query',
      '--file',
      source,
      '--where',
      JSON.stringify(input.where),
      '--json',
    ]);
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
    const run = typedCli(work, ['query', '--file', source, '--json']);
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
