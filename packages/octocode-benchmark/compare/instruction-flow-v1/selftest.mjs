import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { admit } from './proxy.mjs';
import { grade } from './run.mjs';

const root = mkdtempSync(join(tmpdir(), 'instruction-flow-selftest-'));
try {
  const fixture = join(root, 'fixture'); mkdirSync(fixture);
  const path = join(fixture, 'source.ts'); writeFileSync(path, 'export const key = "verified";');
  const secret = join(root, 'grader.json'); writeFileSync(secret, '{}');
  symlinkSync(secret, join(fixture, 'escape.json'));
  const allowed = ['localFetch', 'clasify'];
  assert.deepEqual(admit('localFetch', { path }, allowed, fixture), { rows: 1, cells: 0 });
  for (const bad of [secret, join(fixture, 'escape.json')]) {
    assert.throws(() => admit('localFetch', { path: bad }, allowed, fixture));
    assert.throws(() => admit('clasify', { resources: [{ context: { tool: 'localFetch', query: { path: bad } } }], questions: [{}] }, allowed, fixture));
  }
  assert.throws(() => admit('ghGetFileContent', {}, allowed, fixture));
  assert.deepEqual(admit('clasify', { resources: [{ context: { value: 'state' } }], questions: [{}] }, allowed, fixture), { rows: 1, cells: 1 });
  const expected = [{ id: 'key', value: 'verified', path, line: 1 }];
  const answer = { items: [{ id: 'key', value: 'verified', evidence: [{ path, line: 1 }] }] };
  const call = { event: 'call', admitted: true, name: 'localFetch', args: { path }, result: {
    structuredContent: { results: [{ index: 0, data: { content: 'export const key = "verified";', sourceLineRanges: [{ start: 1, end: 1 }] } }] } } };
  assert.ok(grade(expected, answer, [call]));
  assert.equal(grade(expected, answer, []), false);
  assert.equal(grade(expected, answer, [{ ...call, name: 'clasify' }]), false);
  assert.equal(grade(expected, { items: [{ ...answer.items[0], value: 'wrong' }] }, [call]), false);
  assert.equal(grade(expected, { items: [{ ...answer.items[0], evidence: [{ path, line: 2 }] }] }, [call]), false);
  assert.equal(grade(expected, { items: [answer.items[0], answer.items[0]] }, [call]), false);
  console.log('Instruction-flow scope and deterministic grading selftests passed.');
} finally { rmSync(root, { recursive: true, force: true }); }
