import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { Checkpoints } from '../src/files/checkpoint.js';
import { inheritCheckpoints, sessionIdOf } from '../src/files/checkpoint-store.js';
import { sha256 } from '../src/shared/atomic.js';
import { change } from './checkpoint-change.js';
import { tmp } from './helpers.js';

const journalLines = (store: string) => fs.readFileSync(path.join(store, 'journal.jsonl'), 'utf8').split('\n').filter(Boolean);

const journalEntry = (after: string) => `${JSON.stringify({ turn: 1, path: '/x', before: null, after, bytes: 0 })}\n`;

describe('checkpoint journal', () => {
  it('appends one journal line per settled change, keeps the last line per file and turn, and repairs a torn last line', async () => {
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'v0');
    const store = path.join(tmp(), 'cp');
    const checkpoints = new Checkpoints();
    checkpoints.open(store);
    checkpoints.beginTurn();
    const rewrites = vi.spyOn(fs, 'fsyncSync');
    try {
      await change(checkpoints, file, () => fs.writeFileSync(file, 'v1'));
      await change(checkpoints, file, () => fs.writeFileSync(file, 'v2'));
      // Settling syncs nothing and rewrites nothing: the journal only grows.
      expect(rewrites).not.toHaveBeenCalled();
    } finally {
      rewrites.mockRestore();
    }
    expect(journalLines(store)).toHaveLength(2);
    // A crash tore the next line; an unsettled line is ignored too.
    fs.appendFileSync(path.join(store, 'journal.jsonl'), `${JSON.stringify({ turn: 1, path: 'x', before: null, bytes: 0 })}\n{"turn":1,"pa`);
    const reopened = new Checkpoints();
    reopened.open(store);
    expect(reopened.list()).toHaveLength(1);
    expect(journalLines(store)).toHaveLength(1);
    reopened.beginTurn();
    await change(reopened, file, () => fs.writeFileSync(file, 'v3'));
    const again = new Checkpoints();
    again.open(store);
    expect(again.turns()).toEqual([2, 1]);
    expect(await again.rewind([2, 1])).toEqual({ restored: [file], partly: [], skipped: [] });
    expect(fs.readFileSync(file, 'utf8')).toBe('v0');
  });

  it('compacts a journal dominated by superseded lines when it opens', async () => {
    const store = path.join(tmp(), 'cp');
    fs.mkdirSync(store, { recursive: true });
    fs.writeFileSync(path.join(store, 'journal.jsonl'), Array.from({ length: 20 }, (_, index) => journalEntry(`d${index}`)).join(''));
    const checkpoints = new Checkpoints();
    checkpoints.open(store);
    expect(checkpoints.list()).toEqual([{ turn: 1, path: '/x', before: null, after: 'd19', bytes: 0 }]);
    expect(journalLines(store)).toHaveLength(1);
  });
});

describe('checkpoint blobs', () => {
  it('rewrites a torn blob instead of reusing it, and writes blobs and journal without leaving temp files', async () => {
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'original content');
    const store = path.join(tmp(), 'cp');
    const digest = sha256('original content');
    // A crash mid-write left a truncated blob under the right name.
    fs.mkdirSync(path.join(store, 'blobs'), { recursive: true });
    fs.writeFileSync(path.join(store, 'blobs', digest), 'orig');
    const checkpoints = new Checkpoints();
    checkpoints.open(store);
    checkpoints.beginTurn();
    await change(checkpoints, file, () => fs.writeFileSync(file, 'agent'));
    expect(fs.readFileSync(path.join(store, 'blobs', digest), 'utf8')).toBe('original content');
    expect(fs.readdirSync(path.join(store, 'blobs'))).toEqual([digest]);
    const written = fs.readdirSync(store);
    written.sort();
    expect(written).toEqual(['blobs', 'journal.jsonl']);
    expect(await checkpoints.rewind([1])).toEqual({ restored: [file], partly: [], skipped: [] });
    expect(fs.readFileSync(file, 'utf8')).toBe('original content');
  });

  it('reuses an existing blob of the right size without reading it, and skips a restore whose blob is corrupt', async () => {
    const cwd = tmp();
    const [a, b] = [path.join(cwd, 'a.txt'), path.join(cwd, 'b.txt')];
    fs.writeFileSync(a, 'same');
    fs.writeFileSync(b, 'same');
    const store = path.join(tmp(), 'cp');
    const checkpoints = new Checkpoints();
    checkpoints.open(store);
    checkpoints.beginTurn();
    await change(checkpoints, a, () => fs.writeFileSync(a, 'A'));
    const [blob] = fs.readdirSync(path.join(store, 'blobs'));
    const reads = vi.spyOn(fs.promises, 'readFile');
    const writes = vi.spyOn(fs.promises, 'open');
    try {
      const capture = await checkpoints.capture(b);
      fs.writeFileSync(b, 'B');
      await checkpoints.settle(b, capture, true, 'known');
      expect(reads.mock.calls.map(([file]) => file)).toEqual([b]);
      expect(writes).not.toHaveBeenCalled();
    } finally {
      reads.mockRestore();
      writes.mockRestore();
    }
    // Same size, wrong bytes: caught at rewind, not at capture.
    fs.writeFileSync(b, 'B');
    await checkpoints.settle(b, { fresh: false }, true);
    fs.writeFileSync(path.join(store, 'blobs', blob), 'evil');
    expect(await checkpoints.rewind([1])).toEqual({ restored: [], partly: [], skipped: [b, a] });
  });
});

describe('checkpoints across a fork', () => {
  it('finds the turns at or after a fork point on the session branch', async () => {
    const cwd = tmp();
    const checkpoints = new Checkpoints();
    checkpoints.open(path.join(tmp(), 'cp'));
    const file = path.join(cwd, 'a.txt');
    checkpoints.beginTurn();
    await checkpoints.settle(file, await checkpoints.capture(file, () => 'u1'), true);
    checkpoints.beginTurn();
    await checkpoints.settle(file, await checkpoints.capture(file, () => 'u2'), true);
    const branch = ['root', 'u1', 'a1', 'u2', 'a2'];
    expect(checkpoints.turnsFrom(branch, 'u2')).toEqual([2]);
    expect(checkpoints.turnsFrom(branch, 'a1')).toEqual([2]);
    expect(checkpoints.turnsFrom(branch, 'u1')).toEqual([2, 1]);
    expect(checkpoints.turnsFrom(branch, 'elsewhere')).toEqual([]);
  });

  it('seeds a fork with the parent turns on its branch, linking their blobs, once', async () => {
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'v0');
    const root = tmp();
    const parent = new Checkpoints();
    parent.open(path.join(root, 'parent'));
    parent.beginTurn();
    await change(parent, file, () => fs.writeFileSync(file, 'v1'));
    await parent.settle(path.join(cwd, 'b.txt'), await parent.capture(path.join(cwd, 'b.txt'), () => 'u1'), true);
    parent.beginTurn();
    const second = await parent.capture(file, () => 'u2');
    fs.writeFileSync(file, 'v2');
    await parent.settle(file, second, true);
    // Only turn 2 is anchored on the fork's branch (turn 1's first change had no anchor).
    expect(await inheritCheckpoints(path.join(root, 'parent'), path.join(root, 'child'), ['u2'])).toBe(1);
    expect(await inheritCheckpoints(path.join(root, 'parent'), path.join(root, 'child'), ['u2'])).toBe(0);
    expect(await inheritCheckpoints(path.join(root, 'parent'), path.join(root, 'other'), ['nowhere'])).toBe(0);
    const child = new Checkpoints();
    child.open(path.join(root, 'child'));
    expect(child.turns()).toEqual([2]);
    expect(await child.rewind([2])).toEqual({ restored: [file], partly: [], skipped: [] });
    expect(fs.readFileSync(file, 'utf8')).toBe('v1');
    // The parent keeps its own link to the blob.
    expect(fs.readdirSync(path.join(root, 'parent', 'blobs'))).toHaveLength(2);
  });

  it('does not inherit entries whose digests are not 64 hex characters', async () => {
    const root = tmp();
    const from = path.join(root, 'parent');
    fs.mkdirSync(path.join(from, 'blobs'), { recursive: true });
    const digest = sha256('v0');
    fs.writeFileSync(path.join(from, 'blobs', digest), 'v0');
    fs.writeFileSync(path.join(root, 'secret'), 'not a blob');
    const entry = (turn: number, before: unknown, after: unknown = digest) => ({ turn, anchor: 'u1', path: `/w/f${turn}`, before, after, bytes: 2 });
    const lines = [entry(1, digest), entry(2, '../../secret'), entry(3, `${digest}/..`), entry(4, digest.toUpperCase()), entry(5, 'abc'), entry(6, null), entry(7, digest, '../x'), entry(8, 42)];
    fs.writeFileSync(path.join(from, 'journal.jsonl'), lines.map((line) => `${JSON.stringify(line)}\n`).join(''));
    const to = path.join(root, 'child');
    expect(await inheritCheckpoints(from, to, ['u1'])).toBe(2);
    expect(fs.readdirSync(path.join(to, 'blobs'))).toEqual([digest]);
    expect(journalLines(to).map((line): unknown => JSON.parse(line))).toMatchObject([{ turn: 1 }, { turn: 6 }]);
  });
});

describe('checkpoint fork counts and session ids', () => {
  it('counts only the entries whose blob reached the fork', async () => {
    const root = tmp();
    const from = path.join(root, 'parent');
    fs.mkdirSync(path.join(from, 'blobs'), { recursive: true });
    const present = sha256('v0');
    const missing = sha256('gone');
    fs.writeFileSync(path.join(from, 'blobs', present), 'v0');
    const entry = (turn: number, before: string | null) => ({ turn, anchor: 'u1', path: `/w/f${turn}`, before, after: present, bytes: 2 });
    fs.writeFileSync(path.join(from, 'journal.jsonl'), [entry(1, present), entry(2, missing), entry(3, null)].map((line) => `${JSON.stringify(line)}\n`).join(''));
    const to = path.join(root, 'child');
    // Entry 2 stays in the journal (rewind skips it), but its blob was not carried: it is not counted.
    expect(await inheritCheckpoints(from, to, ['u1'])).toBe(2);
    expect(fs.readdirSync(path.join(to, 'blobs'))).toEqual([present]);
    expect(journalLines(to)).toHaveLength(3);
  });

  it('reads the session id from a Pi session file header', async () => {
    const dir = tmp();
    fs.writeFileSync(path.join(dir, 's.jsonl'), `${JSON.stringify({ type: 'session', id: 'abc' })}\n{}\n`);
    fs.writeFileSync(path.join(dir, 'bad.jsonl'), 'nope');
    expect(await sessionIdOf(path.join(dir, 's.jsonl'))).toBe('abc');
    expect(await sessionIdOf(path.join(dir, 'bad.jsonl'))).toBeUndefined();
    expect(await sessionIdOf(path.join(dir, 'missing.jsonl'))).toBeUndefined();
  });
});
