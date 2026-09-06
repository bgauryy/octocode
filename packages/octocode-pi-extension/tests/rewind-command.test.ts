import assert from 'node:assert/strict';
import { test } from 'vitest';
import type { CheckpointInfo } from '../src/tools/checkpoints.js';
import {
  buildCheckpointItems,
  createCheckpointInputHook,
  formatCheckpointList,
  formatDiffStat,
  leafEntryId,
  snapshotLabel,
  type RewindEngine,
} from '../src/tools/rewind-command.js';
import type { PiContext } from '../src/types.js';

// ─── Fakes ───────────────────────────────────────────────────────────────────

function fakeEngine(checkpoints: CheckpointInfo[] = []) {
  const calls = {
    snapshots: [] as Array<{ label: string; entryId?: string }>,
    restores: [] as Array<{ id: string; paths?: string[] }>,
    diffs: [] as string[],
  };
  const engine: RewindEngine = {
    snapshot: async (label, opts) => {
      calls.snapshots.push({ label, entryId: opts?.entryId });
      return { id: 'snap-id', filesChanged: 1 };
    },
    listCheckpoints: async () => checkpoints,
    restoreFiles: async (id, paths) => {
      calls.restores.push({ id, paths });
    },
    diffStat: async (id) => {
      calls.diffs.push(id);
      return [{ status: 'M', path: 'a.txt' }];
    },
  };
  return { engine, calls };
}

const CP1: CheckpointInfo = { id: 'a1b2c3d4e5f60718', label: 'before: fix bug', ts: 1_700_000_000_000, filesChanged: 2, entryId: 'entry-9' };
const CP2: CheckpointInfo = { id: 'ffee00112233aabb', label: '', ts: 1_700_000_100_000, filesChanged: 0 };

function flush(): Promise<void> {
  return new Promise((resolve) => setImmediate(resolve));
}

// ─── Pure helpers ────────────────────────────────────────────────────────────

test('snapshotLabel collapses whitespace and truncates to 40 chars', () => {
  assert.equal(snapshotLabel('  fix   the\n\tthing  '), 'before: fix the thing');
  const long = 'x'.repeat(100);
  assert.equal(snapshotLabel(long), `before: ${'x'.repeat(40)}`);
});

test('buildCheckpointItems carries id as value and marks conversation-capable checkpoints', () => {
  const items = buildCheckpointItems([CP1, CP2]);
  assert.equal(items[0]!.value, CP1.id);
  assert.match(items[0]!.description!, /2 files · a1b2c3d4 · conversation/);
  assert.match(items[1]!.label, /\(no label\)/);
  assert.ok(!items[1]!.description!.includes('conversation'));
});

test('formatCheckpointList and formatDiffStat render human-readable text', () => {
  assert.match(formatCheckpointList([]), /No checkpoints yet/);
  const listed = formatCheckpointList([CP1]);
  assert.match(listed, /a1b2c3d4/);
  assert.match(listed, /before: fix bug/);
  assert.match(formatDiffStat([{ status: 'M', path: 'a.txt' }]), /^M a\.txt$/);
  assert.match(formatDiffStat([]), /No differences/);
});

test('leafEntryId prefers getLeafId, falls back to the last branch entry', () => {
  const withLeaf = { sessionManager: { getLeafId: () => 'leaf-1', getBranch: () => [{ id: 'b1' }] } } as unknown as PiContext;
  assert.equal(leafEntryId(withLeaf), 'leaf-1');
  const branchOnly = { sessionManager: { getBranch: () => [{ id: 'b1' }, { id: 'b2' }] } } as unknown as PiContext;
  assert.equal(leafEntryId(branchOnly), 'b2');
  assert.equal(leafEntryId(undefined), undefined);
});

// ─── Input hook ──────────────────────────────────────────────────────────────

test('input hook snapshots user prompts (fire-and-forget) with the leaf entry id', async () => {
  const { engine, calls } = fakeEngine();
  const hook = createCheckpointInputHook({ getEngine: () => engine });
  const ctx = { sessionManager: { getBranch: () => [{ id: 'e1' }, { id: 'e2' }] } } as unknown as PiContext;

  const res = await hook({ text: 'fix the login bug please', source: 'interactive' }, ctx);
  assert.deepEqual(res, { action: 'continue' });
  await flush();
  assert.equal(calls.snapshots.length, 1);
  assert.equal(calls.snapshots[0]!.label, 'before: fix the login bug please');
  assert.equal(calls.snapshots[0]!.entryId, 'e2');
});

test('input hook skips slash commands, extension-sourced input, steering, and empty text', async () => {
  const { engine, calls } = fakeEngine();
  const hook = createCheckpointInputHook({ getEngine: () => engine });

  assert.deepEqual(await hook({ text: '/octocode-rewind', source: 'interactive' }, undefined), { action: 'continue' });
  assert.deepEqual(await hook({ text: 'do it', source: 'extension' }, undefined), { action: 'continue' });
  assert.deepEqual(await hook({ text: 'steer left', source: 'interactive', streamingBehavior: 'steer' }, undefined), { action: 'continue' });
  assert.deepEqual(await hook({ text: '   ', source: 'interactive' }, undefined), { action: 'continue' });
  await flush();
  assert.equal(calls.snapshots.length, 0);
});

test('input hook never blocks on a slow snapshot', async () => {
  let resolved = false;
  const engine: RewindEngine = {
    snapshot: () => new Promise(() => undefined), // never resolves
    listCheckpoints: async () => [],
    restoreFiles: async () => undefined,
    diffStat: async () => [],
  };
  const hook = createCheckpointInputHook({ getEngine: () => engine });
  const pending = hook({ text: 'slow one', source: 'interactive' }, undefined).then((r) => {
    resolved = true;
    return r;
  });
  const res = await pending;
  assert.equal(resolved, true);
  assert.deepEqual(res, { action: 'continue' });
});
