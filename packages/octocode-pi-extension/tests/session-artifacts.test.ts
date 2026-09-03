import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, test } from 'vitest';
import {
  compareAndSwapPlanProjection,
  createSessionArtifactContext,
  isPathInsideSessionRoot,
  readPlanProjection,
  resolveSessionIdentity,
  workspaceAgentRoot,
  writePlanBranchSnapshot,
  type PlanProjectionV1,
} from '../src/tools/session-artifacts.js';

const roots: string[] = [];
const originalHome = process.env['OCTOCODE_HOME'];

function tempRoot(prefix: string): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  roots.push(root);
  return root;
}

afterEach(() => {
  if (originalHome === undefined) delete process.env['OCTOCODE_HOME'];
  else process.env['OCTOCODE_HOME'] = originalHome;
  for (const root of roots.splice(0)) fs.rmSync(root, { recursive: true, force: true });
});

test('session identity precedence, workspace isolation, and collision-safe keys are deterministic', () => {
  const workspace = tempRoot('octocode-session-identity-');
  const fromId = resolveSessionIdentity({
    cwd: workspace,
    sessionManager: { getSessionId: () => 'session/a', getSessionFile: () => '/ignored/session.jsonl' },
  });
  assert.equal(fromId.source, 'session-id');
  assert.equal(fromId.rawId, 'session/a');
  assert.ok(fromId.sessionKey.startsWith('session-a-'));

  const blankFallsThrough = resolveSessionIdentity({
    cwd: workspace,
    sessionManager: { getSessionId: () => '  ', getSessionFile: () => './sessions/current.jsonl' },
  });
  assert.equal(blankFallsThrough.source, 'session-file');
  assert.equal(blankFallsThrough.rawId, path.resolve('sessions/current.jsonl'));
  assert.ok(!blankFallsThrough.sessionKey.includes(path.sep));

  const otherFile = resolveSessionIdentity({
    cwd: workspace,
    sessionManager: { getSessionFile: () => '/other/sessions/current.jsonl' },
  });
  assert.notEqual(blankFallsThrough.sessionKey, otherFile.sessionKey, 'full session-file path participates in identity');

  const sanitizedCollision = resolveSessionIdentity({ cwd: workspace, sessionManager: { getSessionId: () => 'session_a' } });
  assert.notEqual(fromId.sessionKey, sanitizedCollision.sessionKey, 'hash suffix distinguishes equal readable slugs');

  const otherWorkspace = tempRoot('octocode-session-identity-other-');
  const sameIdOtherWorkspace = resolveSessionIdentity({ cwd: otherWorkspace, sessionManager: { getSessionId: () => 'session/a' } });
  assert.notEqual(fromId.sessionKey, sameIdOtherWorkspace.sessionKey);
});

test('process fallback is stable for one process/workspace and isolated across workspaces', () => {
  const a = tempRoot('octocode-session-fallback-a-');
  const b = tempRoot('octocode-session-fallback-b-');
  const first = resolveSessionIdentity({ cwd: a, processId: 4242 });
  const again = resolveSessionIdentity({ cwd: a, processId: 4242 });
  const other = resolveSessionIdentity({ cwd: b, processId: 4242 });
  assert.equal(first.source, 'process-fallback');
  assert.equal(first.sessionKey, again.sessionKey);
  assert.notEqual(first.sessionKey, other.sessionKey);
});

test('context contains private writes, rejects traversal and symlink escapes, and preserves old bytes on serialization failure', () => {
  const workspace = tempRoot('octocode-session-context-');
  const ctx = createSessionArtifactContext({ cwd: workspace, sessionManager: { getSessionId: () => 'ctx-test' } });
  assert.equal(ctx.root, path.join(workspaceAgentRoot(workspace), 'sessions', ctx.identity.sessionKey));
  assert.equal(ctx.root.startsWith(workspace), false);
  assert.ok(isPathInsideSessionRoot(ctx, ctx.resolve('plan', 'state.json')));
  assert.equal(isPathInsideSessionRoot(ctx, workspace), false);

  assert.throws(() => ctx.resolve('..', 'escape.txt'), /session root|relative|traversal/i);
  assert.throws(() => ctx.resolve(path.resolve(workspace, 'absolute.txt')), /absolute|relative/i);
  assert.throws(() => ctx.resolve('bad\0name'), /NUL/i);
  assert.throws(() => ctx.resolve(''), /empty/i);

  ctx.writeText('plan/state.txt', 'old');
  const statePath = ctx.resolve('plan/state.txt');
  assert.equal(fs.readFileSync(statePath, 'utf8'), 'old');
  ctx.writeBinary('media/stdout.bin', new Uint8Array([0, 1, 2, 255]));
  assert.deepEqual([...fs.readFileSync(ctx.resolve('media/stdout.bin'))], [0, 1, 2, 255]);

  const circular: Record<string, unknown> = {};
  circular['self'] = circular;
  assert.throws(() => ctx.writeJson('plan/state.txt', circular));
  assert.equal(fs.readFileSync(statePath, 'utf8'), 'old', 'failed serialization never replaces the destination');
  assert.equal(fs.readdirSync(path.dirname(statePath)).some((name) => name.includes('.tmp-')), false);

  if (process.platform !== 'win32') {
    assert.equal(fs.statSync(ctx.root).mode & 0o777, 0o700);
    assert.equal(fs.statSync(statePath).mode & 0o777, 0o600);
    const outside = tempRoot('octocode-session-outside-');
    const link = ctx.resolve('linked');
    fs.symlinkSync(outside, link, 'dir');
    assert.throws(() => ctx.writeText('linked/escape.txt', 'nope'), /symlink|session root|escape/i);
    assert.equal(fs.existsSync(path.join(outside, 'escape.txt')), false);
  }
});

test('context rejects a symlinked private extension root', () => {
  if (process.platform === 'win32') return;
  const workspace = tempRoot('octocode-session-root-symlink-');
  const outside = tempRoot('octocode-session-root-outside-');
  const octocodeHome = tempRoot('octocode-session-home-');
  fs.symlinkSync(outside, path.join(octocodeHome, 'extension'), 'dir');
  assert.throws(
    () => createSessionArtifactContext({ cwd: workspace, octocodeHome, sessionManager: { getSessionId: () => 'escape-test' } }),
    /root must not be a symlink/i,
  );
  assert.deepEqual(fs.readdirSync(outside), []);
});

test('manifest tracks bounded producer paths and NDJSON events append complete private lines', () => {
  const workspace = tempRoot('octocode-session-manifest-');
  const ctx = createSessionArtifactContext({ cwd: workspace, sessionManager: { getSessionId: () => 'manifest-test' } });
  ctx.registerProducer('plan', 'plan/state.json');
  ctx.registerProducer('plan', 'plan/state.json');
  ctx.registerProducer('compaction', 'compaction/first.md');

  const manifest = ctx.inspect()!;
  assert.equal(manifest.version, 1);
  assert.equal(manifest.sessionKey, ctx.identity.sessionKey);
  assert.equal(manifest.identitySource, 'session-id');
  assert.equal('rawId' in manifest, false);
  assert.deepEqual(manifest.producers.plan?.paths, ['plan/state.json']);
  assert.deepEqual(manifest.producers.compaction?.paths, ['compaction/first.md']);

  ctx.appendEvent('plan/events.jsonl', { sequence: 1, type: 'review.accept' });
  ctx.appendEvent('plan/events.jsonl', { sequence: 2, type: 'implementation.start' });
  const lines = fs.readFileSync(ctx.resolve('plan/events.jsonl'), 'utf8').trimEnd().split('\n');
  assert.deepEqual(lines.map((line) => JSON.parse(line)), [
    { sequence: 1, type: 'review.accept' },
    { sequence: 2, type: 'implementation.start' },
  ]);
  if (process.platform !== 'win32') assert.equal(fs.statSync(ctx.resolve('plan/events.jsonl')).mode & 0o777, 0o600);
});

test('branch snapshots are immutable/idempotent and projection CAS rejects stale generations', () => {
  const workspace = tempRoot('octocode-session-cas-');
  const input = { cwd: workspace, sessionManager: { getSessionId: () => 'cas-test' } };
  const firstCtx = createSessionArtifactContext(input);
  const secondCtx = createSessionArtifactContext(input);

  const snapshot = { version: 1 as const, sourceEntryId: 'entry/one', generation: 1, capturedAt: new Date(0).toISOString(), state: { phase: 'draft' } };
  const snapshotPath = writePlanBranchSnapshot(firstCtx, snapshot);
  assert.equal(writePlanBranchSnapshot(firstCtx, snapshot), snapshotPath);
  assert.throws(() => writePlanBranchSnapshot(firstCtx, { ...snapshot, state: { phase: 'accepted' } }), /integrity|immutable|conflict/i);

  const generationOne: PlanProjectionV1<{ phase: string }> = { ...snapshot };
  const winner = compareAndSwapPlanProjection(firstCtx, null, generationOne);
  assert.equal(winner.ok, true);

  const competing = compareAndSwapPlanProjection(secondCtx, null, { ...generationOne, sourceEntryId: 'entry/two' });
  assert.deepEqual(competing, { ok: false, reason: 'generation-conflict', actualGeneration: 1 });
  assert.deepEqual(readPlanProjection(firstCtx), generationOne);

  const stale = compareAndSwapPlanProjection(firstCtx, 0, { ...generationOne, generation: 2 });
  assert.deepEqual(stale, { ok: false, reason: 'generation-conflict', actualGeneration: 1 });

  const generationTwo: PlanProjectionV1<{ phase: string }> = {
    ...generationOne,
    sourceEntryId: 'entry/two',
    generation: 2,
    capturedAt: new Date(1).toISOString(),
    state: { phase: 'accepted' },
  };
  assert.equal(compareAndSwapPlanProjection(firstCtx, 1, generationTwo).ok, true);
  assert.deepEqual(readPlanProjection(firstCtx), generationTwo);
});

test('an old-looking lock is never reclaimed and leaves projection state unchanged', () => {
  const workspace = tempRoot('octocode-session-lock-');
  const ctx = createSessionArtifactContext({ cwd: workspace, sessionManager: { getSessionId: () => 'lock-test' } });
  const lockPath = ctx.resolve('plan/state.json.lock');
  fs.mkdirSync(path.dirname(lockPath), { recursive: true });
  fs.writeFileSync(lockPath, `${JSON.stringify({ token: 'other-owner', pid: 1, createdAt: new Date(0).toISOString() })}\n`, { mode: 0o600 });
  const next = { version: 1 as const, sourceEntryId: 'entry', generation: 1, capturedAt: new Date(0).toISOString(), state: { phase: 'draft' } };
  assert.throws(() => compareAndSwapPlanProjection(ctx, null, next), /Timed out waiting for session artifact lock/);
  assert.equal(fs.existsSync(lockPath), true, 'contention never deletes another owner lock');
  assert.equal(readPlanProjection(ctx), undefined);
});
