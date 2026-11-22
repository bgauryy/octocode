import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { tempDir } from './test-tmp.mjs';
import { runQuery } from '../src/runtime/query.mjs';

test('enrich binds providers to the exact export, preserves revision and rejects replacement', () => {
  const dir = tempDir('graph-facts-'),
    base = join(dir, 'data.json'),
    manifest = join(dir, 'sources.json'),
    out = join(dir, 'enriched.json');
  const data = JSON.stringify({
    section: '/repo',
    files: [{ id: 'a.rs' }],
    symbols: [],
    imports: [],
    calls: [],
  });
  writeFileSync(base, data);
  const digest = bytes => createHash('sha256').update(bytes).digest('hex');
  const source = JSON.stringify({
    documents: [
      {
        relativePath: 'a.rs',
        occurrences: [
          { symbol: 'local 0', symbolRoles: 1, range: [1, 0, 3] },
          { symbol: 'local 0', symbolRoles: 8, range: [4, 0, 3] },
        ],
      },
    ],
  });
  writeFileSync(join(dir, 'scip.json'), source);
  const spec = {
    baseSha256: digest(data),
    revision: 'build-1',
    providers: [
      {
        format: 'scip-json',
        file: 'scip.json',
        sha256: digest(source),
        revision: 'build-1',
      },
    ],
  };
  const run = () =>
    spawnSync(
      process.execPath,
      [
        new URL('../bin/octocode-graph.mjs', import.meta.url).pathname,
        'enrich',
        base,
        manifest,
        out,
      ],
      { encoding: 'utf8' }
    );
  writeFileSync(manifest, JSON.stringify(spec));
  const good = run();
  assert.equal(good.status, 0, good.stderr);
  const enriched = JSON.parse(readFileSync(out, 'utf8'));
  const answer = runQuery(enriched, 'trace a.rs READS');
  assert.equal(answer.ok, true, answer.error);
  assert.deepEqual(
    answer.steps.map(step => step.lines),
    [[5]]
  );
  assert.equal(answer.revision, 'build-1');
  assert.equal(run().status, 1);
  const saved = readFileSync(out, 'utf8');
  spec.providers[0].revision = 'other';
  writeFileSync(manifest, JSON.stringify(spec));
  assert.match(run().stderr, /revision/i);
  assert.equal(readFileSync(out, 'utf8'), saved);
  assert.equal(readFileSync(base, 'utf8'), data);
});

test('trace preserves observed spans and positive per-test segments, including provenance', async () => {
  const { enrichExport } = await import('../dist/analysis.js');
  const dir = tempDir('graph-observations-');
  const write = (name, value) => {
    const text = JSON.stringify(value);
    writeFileSync(join(dir, name), text);
    return createHash('sha256').update(text).digest('hex');
  };
  const revision = 'rev-123';
  const baseSha256 = write('base.json', {
    section: '/source',
    files: [{ id: 'a.c' }],
  });
  const coverage = {
    type: 'llvm.coverage.json.export',
    data: [
      {
        files: [
          {
            filename: '/source/a.c',
            segments: [
              [2, 3, 1, true, true, false],
              [3, 1, 0, true, true, false],
              [4, 1, 1, true, true, true],
            ],
          },
        ],
      },
    ],
  };
  const traceId = 'a'.repeat(32),
    parent = '1'.repeat(16),
    child = '2'.repeat(16);
  const span = (spanId, extra) => ({
    traceId,
    spanId,
    startTimeUnixNano: '1700000000000000000',
    endTimeUnixNano: '1700000000000000123',
    ...extra,
  });
  const trace = {
    resourceSpans: [
      {
        scopeSpans: [
          {
            spans: [
              span(parent, {}),
              span(child, {
                parentSpanId: parent,
                attributes: [
                  {
                    key: 'code.file.path',
                    value: { stringValue: '/source/a.c' },
                  },
                ],
              }),
            ],
          },
        ],
      },
    ],
  };
  const providers = [
    {
      file: 'coverage.json',
      format: 'llvm-cov',
      revision,
      testId: 'case',
      sha256: write('coverage.json', coverage),
    },
    {
      file: 'trace.json',
      format: 'otlp-json',
      revision,
      workload: 'example',
      sha256: write('trace.json', trace),
    },
  ];
  const run = () =>
    enrichExport(
      join(dir, 'base.json'),
      join(dir, 'manifest.json'),
      join(dir, 'out.json')
    );
  const manifest = () =>
    write('manifest.json', { revision, baseSha256, providers });
  manifest();
  write('trace.json', {});
  await assert.rejects(run(), /digest mismatch/);
  providers[1].sha256 = write('trace.json', { resourceSpans: 'invalid' });
  manifest();
  await assert.rejects(run(), /array for resourceSpans/);
  providers[1].sha256 = write('trace.json', trace);
  manifest();
  await run();
  const data = JSON.parse(readFileSync(join(dir, 'out.json'), 'utf8'));
  const observed = runQuery(
    data,
    'trace workload:example OBSERVED PARENT_OF EXECUTES_FILE'
  );
  assert.deepEqual(
    observed.rows.map(r => r.endpoint),
    ['a.c']
  );
  assert.deepEqual(
    observed.steps.map(r => r.details.durationNs),
    ['123', '123', '123']
  );
  const covered = runQuery(data, 'trace test:case COVERS');
  assert.deepEqual(
    covered.steps.map(e => [e.target, e.lines, e.artifact]),
    [['a.c', [2], 'coverage.json']]
  );
});

test('typed-path parentheses do not consume the JS call stack', async () => {
  const { RelationIndex, traceRelations } = await import('../dist/analysis.js');
  const index = new RelationIndex(
    ['X'],
    [{ source: 'a', relation: 'X', target: 'b', path: 'a', lines: [] }]
  );
  assert.deepEqual(
    traceRelations(index, 'a', `${'('.repeat(15000)}X${')'.repeat(15000)}`)
      .endpoints,
    ['b']
  );
});

test('compiler graphs join by module bytes, function and block label across different DOT IDs', async () => {
  const { enrichExport } = await import('../dist/analysis.js');
  const dir = tempDir('graph-ir-');
  const write = (name, value) => {
    const text = typeof value === 'string' ? value : JSON.stringify(value);
    writeFileSync(join(dir, name), text);
    return createHash('sha256').update(text).digest('hex');
  };
  const moduleSha256 = write('module.ll', '; fixture module\n'),
    revision = 'build';
  const baseSha256 = write('graph.json', { section: '/repo', files: [] });
  const provider = (title, file, first, second) => {
    const sha256 = write(file, {
      directed: true,
      name: `${title} for 'choose' function`,
      objects: [
        { _gvid: first, label: '{entry}', shape: 'record' },
        { _gvid: second, label: '{exit}', shape: 'record' },
      ],
      edges: [{ tail: first, head: second }],
    });
    return {
      format: 'llvm-graphviz',
      file,
      sha256,
      revision,
      moduleSha256,
      moduleFile: 'module.ll',
    };
  };
  const providers = [
    provider('CFG', 'cfg.json', 1, 2),
    provider('Dominator tree', 'dom.json', 7, 9),
  ];
  write('manifest.json', { providers, baseSha256, revision });
  const run = () =>
    enrichExport(
      join(dir, 'graph.json'),
      join(dir, 'manifest.json'),
      join(dir, 'out.json')
    );
  await run();
  const graph = JSON.parse(readFileSync(join(dir, 'out.json'), 'utf8'));
  const answer = runQuery(
    graph,
    `trace llvm:${moduleSha256}/choose/entry (BRANCHES_TO|IMMEDIATELY_DOMINATES)`
  );
  assert.deepEqual(
    answer.rows.map(r => r.endpoint),
    [`llvm:${moduleSha256}/choose/exit`]
  );
  assert.deepEqual(answer.steps.map(r => r.artifact).sort(), [
    'cfg.json',
    'dom.json',
  ]);
  write('module.ll', 'different module');
  await assert.rejects(run(), /module digest mismatch/);
});
