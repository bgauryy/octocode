import assert from 'node:assert/strict';
import { test } from 'vitest';
import type { CheckpointInfo } from '../src/tools/checkpoints.js';
import {
  buildCheckpointItems,
  createCheckpointInputHook,
  formatCheckpointList,
  formatDiffStat,
  leafEntryId,
  registerRewindCommand,
  snapshotLabel,
  type RewindEngine,
} from '../src/tools/rewind-command.js';
import type { PiCommandContext, PiContext, PiInstance } from '../src/types.js';

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

function fakePi() {
  const commands = new Map<string, {
    handler(args: string, ctx: PiCommandContext): Promise<void>;
    getArgumentCompletions?(prefix: string): Array<{ value: string }> | null;
  }>();
  const pi = {
    registerCommand: (name: string, def: unknown) => commands.set(name, def as never),
  } as unknown as PiInstance;
  return { pi, commands };
}

function notifier() {
  const msgs: Array<{ msg: string; level?: string }> = [];
  return {
    msgs,
    notify: (_ctx: PiContext | undefined, msg: string, level?: string) => {
      msgs.push({ msg, level });
    },
  };
}

/** Overlay fake: returns queued answers in order. */
function overlayQueue(answers: Array<string | null | undefined>) {
  const seen: string[] = [];
  return {
    seen,
    run: async (_ctx: PiContext | undefined, opts: { title: string }) => {
      seen.push(opts.title);
      return answers.shift();
    },
  };
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

// ─── /octocode-rewind ────────────────────────────────────────────────────────

test('registers /octocode-rewind with arg completions', () => {
  const { pi, commands } = fakePi();
  registerRewindCommand(pi, { getEngine: () => fakeEngine().engine });
  const def = commands.get('octocode-rewind');
  assert.ok(def);
  const completions = def.getArgumentCompletions?.('li');
  assert.deepEqual(completions?.map((c) => c.value), ['list']);
  assert.equal(def.getArgumentCompletions?.('zzz'), null);
});

test('"list" arg prints the checkpoint list through notify', async () => {
  const { pi, commands } = fakePi();
  const { engine } = fakeEngine([CP1]);
  const n = notifier();
  registerRewindCommand(pi, { getEngine: () => engine, notify: n.notify });
  await commands.get('octocode-rewind')!.handler('list', {} as PiCommandContext);
  assert.equal(n.msgs.length, 1);
  assert.match(n.msgs[0]!.msg, /a1b2c3d4/);
  assert.match(n.msgs[0]!.msg, /before: fix bug/);
});

test('"restore <id>" restores files directly', async () => {
  const { pi, commands } = fakePi();
  const { engine, calls } = fakeEngine([CP1]);
  const n = notifier();
  registerRewindCommand(pi, { getEngine: () => engine, notify: n.notify });
  await commands.get('octocode-rewind')!.handler(`restore ${CP1.id}`, {} as PiCommandContext);
  assert.deepEqual(calls.restores, [{ id: CP1.id, paths: undefined }]);
  assert.match(n.msgs[0]!.msg, /Files restored from checkpoint a1b2c3d4/);
});

test('unknown args print usage; missing engine degrades with a warning', async () => {
  const { pi, commands } = fakePi();
  const n = notifier();
  registerRewindCommand(pi, { getEngine: () => fakeEngine().engine, notify: n.notify });
  await commands.get('octocode-rewind')!.handler('bogus stuff', {} as PiCommandContext);
  assert.match(n.msgs[0]!.msg, /Usage: \/octocode-rewind/);

  const { pi: pi2, commands: commands2 } = fakePi();
  const n2 = notifier();
  registerRewindCommand(pi2, { getEngine: () => undefined, notify: n2.notify });
  await commands2.get('octocode-rewind')!.handler('', {} as PiCommandContext);
  assert.match(n2.msgs[0]!.msg, /Checkpoints unavailable/);
  assert.equal(n2.msgs[0]!.level, 'warning');
});

test('overlay flow: pick checkpoint then "restore files + rewind conversation" calls navigateTree', async () => {
  const { pi, commands } = fakePi();
  const { engine, calls } = fakeEngine([CP1, CP2]);
  const n = notifier();
  const overlay = overlayQueue([CP1.id, 'restore-rewind']);
  registerRewindCommand(pi, { getEngine: () => engine, notify: n.notify, runOverlay: overlay.run });

  const navigations: Array<{ targetId: string; opts?: unknown }> = [];
  const ctx = {
    hasUI: true,
    navigateTree: async (targetId: string, opts?: unknown) => {
      navigations.push({ targetId, opts });
      return { cancelled: false };
    },
  } as unknown as PiCommandContext;

  await commands.get('octocode-rewind')!.handler('', ctx);
  assert.deepEqual(calls.restores, [{ id: CP1.id, paths: undefined }]);
  assert.equal(navigations.length, 1);
  assert.equal(navigations[0]!.targetId, 'entry-9');
  assert.match(n.msgs.at(-1)!.msg, /conversation rewound/);
  assert.equal(overlay.seen.length, 2, 'two overlay stages shown');
});

test('overlay flow degrades to files-only when navigateTree is absent', async () => {
  const { pi, commands } = fakePi();
  const { engine, calls } = fakeEngine([CP1]);
  const n = notifier();
  const overlay = overlayQueue([CP1.id, 'restore-rewind']);
  registerRewindCommand(pi, { getEngine: () => engine, notify: n.notify, runOverlay: overlay.run });

  await commands.get('octocode-rewind')!.handler('', { hasUI: true } as PiCommandContext);
  assert.deepEqual(calls.restores, [{ id: CP1.id, paths: undefined }]);
  assert.match(n.msgs.at(-1)!.msg, /conversation left in place/);
  assert.equal(n.msgs.at(-1)!.level, 'warning');
});

test('overlay flow degrades to files-only when the checkpoint has no entry id', async () => {
  const { pi, commands } = fakePi();
  const { engine, calls } = fakeEngine([CP2]);
  const n = notifier();
  const overlay = overlayQueue([CP2.id, 'restore-rewind']);
  registerRewindCommand(pi, { getEngine: () => engine, notify: n.notify, runOverlay: overlay.run });

  const ctx = { hasUI: true, navigateTree: async () => ({ cancelled: false }) } as unknown as PiCommandContext;
  await commands.get('octocode-rewind')!.handler('', ctx);
  assert.deepEqual(calls.restores, [{ id: CP2.id, paths: undefined }]);
  assert.match(n.msgs.at(-1)!.msg, /no conversation entry was recorded/);
});

test('overlay flow: "show diff" notifies name-status lines without restoring', async () => {
  const { pi, commands } = fakePi();
  const { engine, calls } = fakeEngine([CP1]);
  const n = notifier();
  const overlay = overlayQueue([CP1.id, 'show-diff']);
  registerRewindCommand(pi, { getEngine: () => engine, notify: n.notify, runOverlay: overlay.run });

  await commands.get('octocode-rewind')!.handler('', { hasUI: true } as PiCommandContext);
  assert.deepEqual(calls.diffs, [CP1.id]);
  assert.equal(calls.restores.length, 0);
  assert.match(n.msgs.at(-1)!.msg, /M a\.txt/);
});

test('overlay cancel (null) and headless (undefined) do nothing destructive', async () => {
  const { pi, commands } = fakePi();
  const { engine, calls } = fakeEngine([CP1]);
  const n = notifier();
  const cancelOverlay = overlayQueue([null]);
  registerRewindCommand(pi, { getEngine: () => engine, notify: n.notify, runOverlay: cancelOverlay.run });
  await commands.get('octocode-rewind')!.handler('', { hasUI: true } as PiCommandContext);
  assert.equal(calls.restores.length, 0);
  assert.equal(n.msgs.length, 0);

  const { pi: pi2, commands: commands2 } = fakePi();
  const n2 = notifier();
  const headless = overlayQueue([undefined]);
  registerRewindCommand(pi2, { getEngine: () => engine, notify: n2.notify, runOverlay: headless.run });
  await commands2.get('octocode-rewind')!.handler('', {} as PiCommandContext);
  assert.equal(calls.restores.length, 0);
  assert.match(n2.msgs[0]!.msg, /No interactive UI/);

  // Second-stage cancel after a valid pick also does nothing.
  const { pi: pi3, commands: commands3 } = fakePi();
  const n3 = notifier();
  const stageCancel = overlayQueue([CP1.id, 'cancel']);
  registerRewindCommand(pi3, { getEngine: () => engine, notify: n3.notify, runOverlay: stageCancel.run });
  await commands3.get('octocode-rewind')!.handler('', { hasUI: true } as PiCommandContext);
  assert.equal(calls.restores.length, 0);
});

test('empty checkpoint list short-circuits before any overlay', async () => {
  const { pi, commands } = fakePi();
  const { engine } = fakeEngine([]);
  const n = notifier();
  const overlay = overlayQueue(['should-not-be-used']);
  registerRewindCommand(pi, { getEngine: () => engine, notify: n.notify, runOverlay: overlay.run });
  await commands.get('octocode-rewind')!.handler('', { hasUI: true } as PiCommandContext);
  assert.equal(overlay.seen.length, 0);
  assert.match(n.msgs[0]!.msg, /No checkpoints yet/);
});
