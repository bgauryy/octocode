import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildReport, buildManifest } from '../scripts/lib/report/render.mjs';

function report(t, body) {
  const dir = mkdtempSync(join(tmpdir(), 'report-widgets-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const file = join(dir, 'report.md');
  writeFileSync(file, '# Example\n\n' + body);
  return file;
}

test('invalid widgets produce actionable build warnings outside skill folders', t => {
  const file = report(t, '```graph\n{"title":"Broken","nodes":[{"id":"a","label":"A"}],"edges":[{"source":"a","target":"missing"}]}\n```\n\n```chart\n{"title":"Broken","type":"bar","labels":["A"],"series":[{"name":"Count","values":[]}]}\n```');
  const result = buildReport(file);
  assert.equal(result.warnings.length, 2);
  assert.match(result.warnings[0], /graph widget 1: Every edge/);
  assert.match(result.warnings[1], /chart widget 2: Every series/);
});

test('graph evidence survives manifest extraction without treating labels as links', t => {
  const file = report(t, '```graph\n{"title":"Graph","nodes":[{"id":"a","label":"<a href=\\"https://fake.example\\">Label</a>","url":"https://example.com/?a=1&b=2"}],"edges":[]}\n```');
  assert.deepEqual(buildReport(file).warnings, []);
  assert.deepEqual(buildManifest(file).documents[0].links, ['https://example.com/?a=1&b=2']);
});
