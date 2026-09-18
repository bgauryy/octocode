import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { resolveEvidenceRefs, resolveRef } from './resolve-content-ref.mjs';

async function fixtureDir(files) {
  const dir = await mkdtemp(join(tmpdir(), 'contentref-'));
  for (const [name, body] of Object.entries(files)) await writeFile(join(dir, name), body);
  return dir;
}

const SAMPLE = ['line one', 'line two anchor', 'line three', 'line four', 'line five'].join('\n');

test('resolves a line span with a source anchor', async () => {
  const dir = await fixtureDir({ 'a.txt': SAMPLE });
  try {
    const { content, source } = resolveRef({ path: 'a.txt', lines: '2-3' }, dir);
    assert.equal(content, 'line two anchor\nline three');
    assert.equal(source, 'a.txt:L2-L3');
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test('resolves a regex window', async () => {
  const dir = await fixtureDir({ 'a.txt': SAMPLE });
  try {
    const { content, source } = resolveRef({ path: 'a.txt', regex: 'anchor', window: [1, 1] }, dir);
    assert.equal(content, 'line one\nline two anchor\nline three');
    assert.equal(source, 'a.txt:L1-L3');
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test('truncates to maxChars and marks source', async () => {
  const dir = await fixtureDir({ 'a.txt': SAMPLE });
  try {
    const { content, source } = resolveRef({ path: 'a.txt', lines: '1-5', maxChars: 5 }, dir);
    assert.equal(content.length, 5);
    assert.match(source, /truncated/);
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test('redacts obvious secrets', async () => {
  const dir = await fixtureDir({ 's.txt': 'token=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123 end' });
  try {
    const { content } = resolveRef({ path: 's.txt', lines: '1' }, dir);
    assert.match(content, /«redacted-token»/);
    assert.doesNotMatch(content, /ABCDEFGHIJKLMNOPQRSTUVWXYZ/);
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test('rejects path traversal and absolute paths', async () => {
  const dir = await fixtureDir({ 'a.txt': SAMPLE });
  try {
    assert.throws(() => resolveRef({ path: '../escape.txt', lines: '1' }, dir), /escapes the allowed sandbox/);
    assert.throws(() => resolveRef({ path: '/etc/hosts', lines: '1' }, dir), /absolute/);
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test('enforces allowedRoots sandbox (R3)', async () => {
  const inside = await fixtureDir({ 'a.txt': SAMPLE });
  const outside = await fixtureDir({ 'b.txt': SAMPLE });
  try {
    // rootDir points at `outside`, but only `inside` is allowlisted -> reject
    assert.throws(() => resolveRef({ path: 'b.txt', lines: '1' }, outside, [inside]), /escapes the allowed sandbox/);
    // a second allowlisted root admits it
    const ok = resolveRef({ path: 'b.txt', lines: '1' }, outside, [inside, outside]);
    assert.equal(ok.content, 'line one');
  } finally {
    await rm(inside, { recursive: true, force: true });
    await rm(outside, { recursive: true, force: true });
  }
});

test('walks state.evidence and fills content, keeping backward compat', async () => {
  const dir = await fixtureDir({ 'a.txt': SAMPLE });
  try {
    const input = {
      route: 'disputed_inference',
      state: {
        evidence: [
          { id: 'E1', scope: 's', contentRef: { path: 'a.txt', lines: '2' } },
          { id: 'E2', scope: 's', source: 'manual', content: 'already inline' } // untouched
        ]
      }
    };
    const { input: out, stats } = resolveEvidenceRefs(input, { rootDir: dir });
    assert.equal(stats.refsResolved, 1);
    assert.equal(out.state.evidence[0].content, 'line two anchor');
    assert.equal(out.state.evidence[0].source, 'a.txt:L2-L2');
    assert.equal(out.state.evidence[0].contentRef, undefined);
    assert.deepEqual(out.state.evidence[1], { id: 'E2', scope: 's', source: 'manual', content: 'already inline' });
    assert.notEqual(out, input); // did not mutate caller input
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test('rejects supplying both content and contentRef', async () => {
  const dir = await fixtureDir({ 'a.txt': SAMPLE });
  try {
    const input = { state: { evidence: [{ id: 'E1', content: 'x', contentRef: { path: 'a.txt', lines: '1' } }] } };
    assert.throws(() => resolveEvidenceRefs(input, { rootDir: dir }), /both content and contentRef/);
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test('resolves state.newEvidence for reflection_delta', async () => {
  const dir = await fixtureDir({ 'a.txt': SAMPLE });
  try {
    const input = { state: { newEvidence: { id: 'E9', scope: 's', contentRef: { path: 'a.txt', lines: '4' } } } };
    const { input: out, stats } = resolveEvidenceRefs(input, { rootDir: dir });
    assert.equal(stats.refsResolved, 1);
    assert.equal(out.state.newEvidence.content, 'line four');
  } finally { await rm(dir, { recursive: true, force: true }); }
});
