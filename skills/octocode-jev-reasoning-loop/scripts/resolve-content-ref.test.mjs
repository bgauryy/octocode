import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm, symlink } from 'node:fs/promises';
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

test('rejects an oversized selected span even with a manual source label', async () => {
  const dir = await fixtureDir({ 'a.txt': SAMPLE });
  try {
    assert.throws(() => resolveEvidenceRefs({ state: { evidence: [{
      id: 'E1', source: 'manual', contentRef: { path: 'a.txt', lines: '1-5', maxChars: 5 }
    }] } }, { rootDir: dir }), /exceeds maxChars.*No evidence was sent/);
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test('rejects symlinks outside the allowed root and accepts links within it', async () => {
  const inside = await fixtureDir({ 'a.txt': SAMPLE });
  const outside = await fixtureDir({ 'secret.txt': 'private' });
  try {
    await symlink(join(outside, 'secret.txt'), join(inside, 'escape'));
    await symlink(join(inside, 'a.txt'), join(inside, 'local'));
    assert.throws(() => resolveRef({ path: 'escape', lines: '1' }, inside), /sandbox through a symlink/);
    assert.equal(resolveRef({ path: 'local', lines: '1' }, inside).content, 'line one');
  } finally {
    await rm(inside, { recursive: true, force: true });
    await rm(outside, { recursive: true, force: true });
  }
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

// Synthetic controls cover the shared provider payload ingress, not only redact().
test('AWS IDs and URL credentials are removed from outgoing evidence payloads', async () => {
  const { buildProfileRequests } = await import('./profile.mjs');
  const { runScout } = await import('./scout.mjs');
  const secrets = ["AKIAABCDEFGHIJKLMNOP", "https://demo:fakepassword@example.invalid/path", "ASIA1234567890ABCDEF", "ABIAABCDEFGHIJKLMNOP", "ACCA0123456789ABCDEF", "postgres://alice:synthetic@db.invalid/test", "HTTP://user:pass%40word@host.invalid:8080/path"];
  const benign = ["https://example.invalid/docs", "alice@example.invalid", "AKIA123456789012345", "AKIA12345678901234567", "https://example.invalid/path:note@tail"];
  const body = [...secrets, ...benign].join('\n');
  const dir = await fixtureDir({ 'source.txt': body, 'other.txt': 'Benign control' });
  try {
    const aspects = [{ key: 'config', type: 'noul', instructions: 'Does this show configuration?' }];
    const profile = buildProfileRequests({ root: dir, inputs: [
      { id: 'local', path: 'source.txt' }, { id: 'inline', content: body }
    ], aspects });
    const localScout = runScout({ root: dir, claim: 'configuration', anchors: ['.'], candidates: ['source.txt', 'other.txt'] }, { dryRun: true });
    const itemScout = runScout({ claim: 'configuration', items: [{ id: 'source', content: body }, { id: 'other', content: 'Benign control' }] }, { dryRun: true });
    const refs = resolveEvidenceRefs({ state: { evidence: [{ id: 'E1', contentRef: { path: 'source.txt', lines: `1-${body.split('\n').length}`, maxChars: 4000 } }] } }, { rootDir: dir });
    const payloads = [
      ...profile.map(row => row.request.state.source.content),
      localScout.request.state.candidates['source.txt'][0].content,
      itemScout.request.state.candidates.source[0].content,
      refs.input.state.evidence[0].content
    ];
    for (const payload of payloads) {
      for (const secret of secrets) assert.equal(payload.includes(secret), false, 'synthetic credential must not reach provider state');
      for (const control of benign) assert.ok(payload.includes(control), `benign control remains: ${control}`);
    }
  } finally { await rm(dir, { recursive: true, force: true }); }
});
