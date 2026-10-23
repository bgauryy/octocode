import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { Checkpoints } from '../src/files/checkpoint.js';
import { sha256 } from '../src/shared/atomic.js';
import { change } from './checkpoint-change.js';
import { tmp } from './helpers.js';

describe('checkpoints', () => {
  it('rewinds the last turn only, newest change first, and persists across reopen', async () => {
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'v0');
    const store = path.join(tmp(), 'cp');
    const checkpoints = new Checkpoints();
    checkpoints.open(store);
    checkpoints.beginTurn();
    await change(checkpoints, file, () => fs.writeFileSync(file, 'v1'));
    await change(checkpoints, file, () => fs.writeFileSync(file, 'v1b'));
    checkpoints.beginTurn();
    await change(checkpoints, file, () => fs.writeFileSync(file, 'v2'));
    expect(checkpoints.list()).toHaveLength(2);

    const reopened = new Checkpoints();
    reopened.open(store);
    expect(reopened.turns()).toEqual([2, 1]);
    expect(await reopened.rewind([2])).toEqual({ restored: [file], partly: [], skipped: [] });
    expect(fs.readFileSync(file, 'utf8')).toBe('v1b');
    expect(await reopened.rewind(reopened.turns().slice(0, 1))).toEqual({ restored: [file], partly: [], skipped: [] });
    expect(fs.readFileSync(file, 'utf8')).toBe('v0');
    expect(reopened.turns()).toEqual([]);
    expect(fs.readdirSync(path.join(store, 'blobs'))).toEqual([]);
  });

  /** A file changed once in each of `versions.length - 1` turns: v0 (on disk before), then each next version. */
  async function chain(versions: string[]) {
    const file = path.join(tmp(), 'a.txt');
    fs.writeFileSync(file, versions[0]!);
    const store = path.join(tmp(), 'cp');
    const checkpoints = new Checkpoints();
    checkpoints.open(store);
    for (const version of versions.slice(1)) {
      checkpoints.beginTurn();
      await change(checkpoints, file, () => fs.writeFileSync(file, version));
    }
    return { file, store, checkpoints };
  }

  it('reaches the first state through a three-turn chain, newest change first', async () => {
    const { file, store, checkpoints } = await chain(['v0', 'v1', 'v2', 'v3']);
    expect(await checkpoints.rewind(checkpoints.turns())).toEqual({ restored: [file], partly: [], skipped: [] });
    expect(fs.readFileSync(file, 'utf8')).toBe('v0');
    expect(checkpoints.list()).toEqual([]);
    expect(fs.readdirSync(path.join(store, 'blobs'))).toEqual([]);
  });

  it('keeps the checkpoint of a restore that failed, and every older one for the path, so a second rewind can retry', async () => {
    const { file, store, checkpoints } = await chain(['v0', 'v1', 'v2', 'v3']);
    const blob = path.join(store, 'blobs', sha256('v1'));
    fs.writeFileSync(blob, 'evil');
    // Turn 3 restores (v2); turn 2's blob is corrupt, so it and turn 1's stay and the path stops there.
    expect(await checkpoints.rewind([3, 2, 1])).toEqual({ restored: [], partly: [file], skipped: [] });
    expect(fs.readFileSync(file, 'utf8')).toBe('v2');
    expect(checkpoints.turns()).toEqual([2, 1]);
    expect(fs.readdirSync(path.join(store, 'blobs')).sort()).toEqual([sha256('v0'), sha256('v1')].sort());
    const reopened = new Checkpoints();
    reopened.open(store);
    expect(reopened.turns()).toEqual([2, 1]);
    // The blob is repaired: the retry carries the file the rest of the way, still newest first.
    fs.writeFileSync(blob, 'v1');
    expect(await reopened.rewind(reopened.turns())).toEqual({ restored: [file], partly: [], skipped: [] });
    expect(fs.readFileSync(file, 'utf8')).toBe('v0');
    expect(reopened.turns()).toEqual([]);
  });

  it('keeps every chosen checkpoint of a file changed since, and restores it once it is as the agent left it', async () => {
    const { file, store, checkpoints } = await chain(['v0', 'v1', 'v2']);
    fs.writeFileSync(file, 'user');
    expect(await checkpoints.rewind([2, 1])).toEqual({ restored: [], partly: [], skipped: [file] });
    expect(fs.readFileSync(file, 'utf8')).toBe('user');
    expect(checkpoints.turns()).toEqual([2, 1]);
    expect(fs.readdirSync(path.join(store, 'blobs')).sort()).toEqual([sha256('v0'), sha256('v1')].sort());
    fs.writeFileSync(file, 'v2');
    expect(await checkpoints.rewind([2, 1])).toEqual({ restored: [file], partly: [], skipped: [] });
    expect(fs.readFileSync(file, 'utf8')).toBe('v0');
  });

  it('rewinds a path that restored cleanly while another one stays', async () => {
    const cwd = tmp();
    const [good, bad] = [path.join(cwd, 'good.txt'), path.join(cwd, 'bad.txt')] as [string, string];
    fs.writeFileSync(good, 'g0');
    fs.writeFileSync(bad, 'b0');
    const checkpoints = new Checkpoints();
    const store = path.join(tmp(), 'cp');
    checkpoints.open(store);
    checkpoints.beginTurn();
    await change(checkpoints, good, () => fs.writeFileSync(good, 'g1'));
    await change(checkpoints, bad, () => fs.writeFileSync(bad, 'b1'));
    fs.writeFileSync(bad, 'user');
    expect(await checkpoints.rewind([1])).toEqual({ restored: [good], partly: [], skipped: [bad] });
    expect(checkpoints.list().map((entry) => entry.path)).toEqual([bad]);
    expect(fs.readdirSync(path.join(store, 'blobs'))).toEqual([sha256('b0')]);
  });

  it('never collects a blob or temp file that a capture in flight is storing', async () => {
    const cwd = tmp();
    const store = path.join(tmp(), 'cp');
    const checkpoints = new Checkpoints();
    checkpoints.open(store);
    checkpoints.beginTurn();
    const files = Array.from({ length: 12 }, (_, index) => path.join(cwd, `f${index}.txt`));
    files.forEach((file, index) => fs.writeFileSync(file, `content ${index}`));
    fs.mkdirSync(path.join(store, 'blobs'), { recursive: true });
    const temp = path.join(store, 'blobs', '.abc.123.tmp');
    fs.writeFileSync(temp, 'partial');
    // Odd files fail (each failure collects garbage) while even captures are still storing their blobs.
    await Promise.all(files.map(async (file, index) => {
      const capture = await checkpoints.capture(file);
      await checkpoints.settle(file, capture, index % 2 === 0);
    }));
    expect(fs.existsSync(temp)).toBe(true);
    for (const entry of checkpoints.list()) expect(fs.existsSync(path.join(store, 'blobs', entry.before!))).toBe(true);
    expect(checkpoints.list()).toHaveLength(6);
  });

  it('leaves files changed since the agent edit alone, removes created files and restores deleted ones with their mode', async () => {
    const cwd = tmp();
    const [edited, created, removed] = ['edited.txt', 'created.txt', 'removed.sh'].map((name) => path.join(cwd, name)) as [string, string, string];
    fs.writeFileSync(edited, 'mine');
    fs.writeFileSync(removed, '#!/bin/sh\n');
    fs.chmodSync(removed, 0o755);
    const checkpoints = new Checkpoints();
    checkpoints.open(path.join(tmp(), 'cp'));
    checkpoints.beginTurn();
    await change(checkpoints, edited, () => fs.writeFileSync(edited, 'agent'));
    await change(checkpoints, created, () => fs.writeFileSync(created, 'new'));
    await change(checkpoints, removed, () => fs.rmSync(removed));
    fs.writeFileSync(edited, 'user edit');
    const result = await checkpoints.rewind([1]);
    expect(result.skipped).toEqual([edited]);
    expect(result.restored.sort()).toEqual([created, removed].sort());
    expect(fs.readFileSync(edited, 'utf8')).toBe('user edit');
    expect(fs.existsSync(created)).toBe(false);
    expect(fs.readFileSync(removed, 'utf8')).toBe('#!/bin/sh\n');
    expect(fs.statSync(removed).mode & 0o777).toBe(0o755);
    expect(fs.readdirSync(cwd).sort()).toEqual(['edited.txt', 'removed.sh']);
  });

  it('deletes a created file on rewind only if it still holds the agent bytes after moving it aside', async () => {
    const cwd = tmp();
    const created = path.join(cwd, 'created.txt');
    const checkpoints = new Checkpoints();
    checkpoints.open(path.join(tmp(), 'cp'));
    checkpoints.beginTurn();
    await change(checkpoints, created, () => fs.writeFileSync(created, 'agent'));
    // Someone rewrites the file between the rewind's check and its move.
    const rename = fs.promises.rename;
    const spy = vi.spyOn(fs.promises, 'rename').mockImplementationOnce(async (from, to) => {
      fs.writeFileSync(from, 'user');
      return rename(from, to);
    });
    try {
      expect(await checkpoints.rewind([1])).toEqual({ restored: [], partly: [], skipped: [created] });
    } finally {
      spy.mockRestore();
    }
    expect(fs.readFileSync(created, 'utf8')).toBe('user');
    expect(fs.readdirSync(cwd)).toEqual(['created.txt']);
  });

  it('forgets a failed change, skips files over 8 MiB and keeps within the entry cap by dropping the oldest turns', async () => {
    const cwd = tmp();
    const checkpoints = new Checkpoints();
    checkpoints.open(path.join(tmp(), 'cp'));
    checkpoints.beginTurn();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'x');
    await checkpoints.settle(file, await checkpoints.capture(file), false);
    expect(checkpoints.list()).toEqual([]);
    const big = path.join(cwd, 'big.bin');
    fs.writeFileSync(big, Buffer.alloc(8 * 1024 * 1024 + 1));
    expect(await checkpoints.capture(big)).toEqual({ fresh: false, note: 'not checkpointed: over 8 MiB' });
    expect(await checkpoints.capture(big, undefined, { data: fs.readFileSync(big), sha256: 'x', mode: 0o644 })).toEqual({ fresh: false, note: 'not checkpointed: over 8 MiB' });
    expect(await checkpoints.capture(cwd)).toEqual({ fresh: false });
    for (let turn = 0; turn < 3; turn++) {
      checkpoints.beginTurn();
      for (let index = 0; index < 100; index++) await change(checkpoints, path.join(cwd, `f${index}.txt`), () => fs.writeFileSync(path.join(cwd, `f${index}.txt`), `${turn}`));
    }
    expect(checkpoints.list().length).toBeLessThanOrEqual(256);
    expect(checkpoints.turns()).toEqual([4, 3]);
  });

  it('never evicts the in-flight turn: past the cap its further files are reported as not checkpointed', async () => {
    const cwd = tmp();
    const checkpoints = new Checkpoints();
    checkpoints.open(path.join(tmp(), 'cp'));
    checkpoints.beginTurn();
    await change(checkpoints, path.join(cwd, 'old.txt'), () => fs.writeFileSync(path.join(cwd, 'old.txt'), 'old'));
    checkpoints.beginTurn();
    const notes: Array<string | undefined> = [];
    for (let index = 0; index < 260; index++) {
      const file = path.join(cwd, `f${index}.txt`);
      const capture = await checkpoints.capture(file);
      notes.push(capture.note);
      fs.writeFileSync(file, 'new');
      await checkpoints.settle(file, capture, true);
    }
    expect(checkpoints.turns()).toEqual([2]);
    expect(checkpoints.list()).toHaveLength(256);
    expect(checkpoints.list()[0]!.path).toBe(path.join(cwd, 'f0.txt'));
    expect(notes.slice(0, 256).every((note) => note === undefined)).toBe(true);
    expect(notes.slice(256)).toEqual(Array(4).fill('not checkpointed: turn over the checkpoint cap'));
  });
});
