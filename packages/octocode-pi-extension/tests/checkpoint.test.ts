import fs from 'node:fs';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Checkpoints } from '../src/files/checkpoint.js';
import { registerCheckpoints } from '../src/files/checkpoint-command.js';
import { inheritCheckpoints, sessionIdOf } from '../src/files/checkpoint-store.js';
import { FileGuard, registerFileTool } from '../src/files/tool.js';
import { tmp } from './helpers.js';
import { Subcommands } from '../src/shared/commands.js';


/** One change through the checkpoint protocol the file tool uses. */
async function change(checkpoints: Checkpoints, file: string, apply: () => void): Promise<void> {
  const capture = await checkpoints.capture(file);
  apply();
  await checkpoints.settle(file, capture, true);
}

const journalLines = (store: string) => fs.readFileSync(path.join(store, 'journal.jsonl'), 'utf8').split('\n').filter(Boolean);

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
    expect(await reopened.rewind([2])).toEqual({ restored: [file], skipped: [] });
    expect(fs.readFileSync(file, 'utf8')).toBe('v1b');
    expect(await reopened.rewind(reopened.turns().slice(0, 1))).toEqual({ restored: [file], skipped: [] });
    expect(fs.readFileSync(file, 'utf8')).toBe('v0');
    expect(reopened.turns()).toEqual([]);
    expect(fs.readdirSync(path.join(store, 'blobs'))).toEqual([]);
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
      expect(await checkpoints.rewind([1])).toEqual({ restored: [], skipped: [created] });
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
    expect(await again.rewind([2, 1])).toEqual({ restored: [file], skipped: [] });
    expect(fs.readFileSync(file, 'utf8')).toBe('v0');
  });

  it('compacts a journal dominated by superseded lines when it opens', async () => {
    const store = path.join(tmp(), 'cp');
    fs.mkdirSync(store, { recursive: true });
    const line = (after: string) => `${JSON.stringify({ turn: 1, path: '/x', before: null, after, bytes: 0 })}\n`;
    fs.writeFileSync(path.join(store, 'journal.jsonl'), Array.from({ length: 20 }, (_, index) => line(`d${index}`)).join(''));
    const checkpoints = new Checkpoints();
    checkpoints.open(store);
    expect(checkpoints.list()).toEqual([{ turn: 1, path: '/x', before: null, after: 'd19', bytes: 0 }]);
    expect(journalLines(store)).toHaveLength(1);
  });

  it('rewrites a torn blob instead of reusing it, and writes blobs and journal without leaving temp files', async () => {
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'original content');
    const store = path.join(tmp(), 'cp');
    const { sha256 } = await import('../src/shared/atomic.js');
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
    expect(fs.readdirSync(store).sort()).toEqual(['blobs', 'journal.jsonl']);
    expect(await checkpoints.rewind([1])).toEqual({ restored: [file], skipped: [] });
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
      expect(reads.mock.calls.map(([file]) => String(file))).toEqual([b]);
      expect(writes).not.toHaveBeenCalled();
    } finally {
      reads.mockRestore();
      writes.mockRestore();
    }
    // Same size, wrong bytes: caught at rewind, not at capture.
    fs.writeFileSync(b, 'B');
    await checkpoints.settle(b, { fresh: false }, true);
    fs.writeFileSync(path.join(store, 'blobs', blob!), 'evil');
    expect(await checkpoints.rewind([1])).toEqual({ restored: [], skipped: [b, a] });
  });

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
    expect(await child.rewind([2])).toEqual({ restored: [file], skipped: [] });
    expect(fs.readFileSync(file, 'utf8')).toBe('v1');
    // The parent keeps its own link to the blob.
    expect(fs.readdirSync(path.join(root, 'parent', 'blobs'))).toHaveLength(2);
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

/** A fake Pi host with the file tool, checkpoints and a branch the test extends. */
function host(cwd: string, sessionId = 'session/1', header?: { parentSession?: string }) {
  const handlers = new Map<string, (event: unknown, ctx: unknown) => Promise<unknown>>();
  const commands = new Subcommands();
  let tool: { execute: (...args: unknown[]) => Promise<unknown> } | undefined;
  const sent: Array<{ message: { customType: string; content: string; display: boolean }; options: unknown }> = [];
  const pi = {
    on: (name: string, handler: (event: unknown, ctx: unknown) => Promise<unknown>) => handlers.set(name, handler),
    registerTool: (definition: typeof tool) => (tool = definition),
    sendMessage: (message: (typeof sent)[number]['message'], options: unknown) => sent.push({ message, options }),
    setLabel: (id: string, label: string | undefined) => (label === undefined ? labels.delete(id) : labels.set(id, label)),
  } as never;
  const labels = new Map<string, string>();
  const guard = new FileGuard();
  const checkpoints = new Checkpoints();
  registerFileTool(pi, guard, undefined, checkpoints);
  const forgotten: string[] = [];
  registerCheckpoints(pi, checkpoints, (target) => forgotten.push(target), commands);
  const branch: Array<{ id: string; type: string; message: { role: string } }> = [{ id: 'u1', type: 'message', message: { role: 'user' } }];
  const notices: string[] = [];
  const selects: string[] = [];
  const ctx = {
    cwd,
    hasUI: true,
    sessionManager: {
      getSessionId: () => sessionId,
      getHeader: () => (header ? { type: 'session', id: sessionId, timestamp: '', cwd, ...header } : null),
      // The branch is one path: each entry's parent is the one before it.
      getBranch: (from?: string) => (from === undefined ? branch : branch.slice(0, branch.findIndex((entry) => entry.id === from) + 1)),
      getEntry: (id: string) => {
        const index = branch.findIndex((entry) => entry.id === id);
        return index < 0 ? undefined : { ...branch[index], parentId: branch[index - 1]?.id ?? null };
      },
      getLabel: (id: string) => labels.get(id),
    },
    ui: { notify: (text: string) => notices.push(text), select: async (title: string, options: string[]) => (selects.push(title), options[0]) },
  };
  const run = (queries: unknown[]) => tool!.execute('t', { queries }, undefined, undefined, ctx);
  const edit = (from: string, to: string) => run([{ reasoning: 'r', type: 'edit', path: 'a.txt', edits: [{ oldText: from, newText: to }] }]);
  return { handlers, commands, guard, forgotten, branch, notices, selects, sent, ctx, run, edit, labels };
}

describe('/rewind and the fork offer', () => {
  const home = process.env['OCTOCODE_HOME'];
  afterEach(() => {
    if (home === undefined) delete process.env['OCTOCODE_HOME'];
    else process.env['OCTOCODE_HOME'] = home;
  });

  it('checkpoints file tool changes per user turn and restores them from /rewind or a fork', async () => {
    process.env['OCTOCODE_HOME'] = tmp();
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'one');
    const { handlers, commands, forgotten, branch, notices, selects, sent, ctx, edit } = host(cwd);
    await handlers.get('session_start')!({}, ctx);
    expect(fs.existsSync(path.join(process.env['OCTOCODE_HOME'], 'agent', 'pi', 'sessions', 'session_1', 'checkpoints'))).toBe(false);

    await handlers.get('agent_start')!({}, ctx);
    await edit('one', 'two');
    branch.push({ id: 'a1', type: 'message', message: { role: 'assistant' } }, { id: 'u2', type: 'message', message: { role: 'user' } });
    // A run woken by a team message or job report is not a turn of its own; the user's next prompt is.
    await handlers.get('input')!({ source: 'extension', text: 'report' }, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await handlers.get('input')!({ source: 'interactive', text: 'next' }, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await edit('two', 'three');
    expect(fs.existsSync(path.join(process.env['OCTOCODE_HOME'], 'agent', 'pi', 'sessions', 'session_1', 'checkpoints', 'journal.jsonl'))).toBe(true);

    await commands.get('rewind')!.handler('', ctx as never);
    expect(fs.readFileSync(file, 'utf8')).toBe('two');
    expect(notices.pop()).toMatch(/Rewound 1 turn: restored 1 file \(a\.txt\)/);
    expect(forgotten).toEqual([file]);
    // The model learns about the rewind on its next turn, without a turn being triggered.
    expect(sent).toEqual([{ message: expect.objectContaining({ customType: 'octocode-rewind', display: false, content: expect.stringMatching(/rewind[\s\S]*a\.txt[\s\S]*read them again/) }), options: { triggerTurn: false } }]);

    // Forking before the first user message offers to restore the files the agent changed after it.
    await handlers.get('session_before_fork')!({ type: 'session_before_fork', entryId: 'u1', position: 'before' }, ctx);
    expect(selects).toEqual(['The agent changed 1 file after this point. Restore them?']);
    // Nothing changes until Pi tears the session down for the fork: it can still refuse the fork.
    expect(fs.readFileSync(file, 'utf8')).toBe('two');
    await handlers.get('session_shutdown')!({ type: 'session_shutdown', reason: 'fork' }, ctx);
    expect(fs.readFileSync(file, 'utf8')).toBe('one');
    await commands.get('rewind')!.handler('3', ctx as never);
    expect(notices.pop()).toBe('No file changes to rewind on this branch.');
    expect(sent).toHaveLength(1);
  });

  it('rewinds only turns on the current branch after a /tree move', async () => {
    process.env['OCTOCODE_HOME'] = tmp();
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'one');
    const { handlers, commands, branch, notices, ctx, edit } = host(cwd);
    await handlers.get('session_start')!({}, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await edit('one', 'two');
    branch.push({ id: 'a1', type: 'message', message: { role: 'assistant' } }, { id: 'u2', type: 'message', message: { role: 'user' } });
    await handlers.get('input')!({ source: 'interactive', text: 'next' }, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await edit('two', 'three');
    // The user moves back to a1 and starts another branch (u3) without restoring: turn 2 (anchored at u2) is off-branch.
    branch.splice(2, 1, { id: 'u3', type: 'message', message: { role: 'user' } });
    await commands.get('rewind')!.handler('', ctx as never);
    expect(notices.pop()).toMatch(/Rewound 1 turn/);
    // Turn 1 (u1) was rewound; turn 2's change from another branch is left: the file had moved on, so it is skipped.
    expect(fs.readFileSync(file, 'utf8')).toBe('three');
    await commands.get('rewind')!.handler('', ctx as never);
    expect(notices.pop()).toBe('No file changes to rewind on this branch.');
  });

  it('labels the user message of each turn that changed files, keeps user labels, and clears on rewind', async () => {
    process.env['OCTOCODE_HOME'] = tmp();
    const cwd = tmp();
    fs.writeFileSync(path.join(cwd, 'a.txt'), 'one');
    const { handlers, commands, branch, ctx, run, labels } = host(cwd);
    await handlers.get('session_start')!({}, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await run([{ reasoning: 'r', type: 'write', path: 'a.txt', content: 'two' }, { reasoning: 'r', type: 'write', path: 'b.txt', content: 'b' }]);
    await handlers.get('agent_end')!({}, ctx);
    expect(labels.get('u1')).toBe('✎ 2 files');
    branch.push({ id: 'a1', type: 'message', message: { role: 'assistant' } }, { id: 'u2', type: 'message', message: { role: 'user' } });
    labels.set('u2', '✎ mine');
    await handlers.get('input')!({ source: 'interactive', text: 'next' }, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await run([{ reasoning: 'r', type: 'write', path: 'a.txt', content: 'three' }]);
    await handlers.get('agent_end')!({}, ctx);
    // A user label that merely starts with the mark is still the user's.
    expect(labels.get('u2')).toBe('✎ mine');
    await commands.get('rewind')!.handler('2', ctx as never);
    expect(labels.has('u1')).toBe(false);
    expect(labels.get('u2')).toBe('✎ mine');
    // A turn whose checkpoints went (pruned) loses its stale label at the next run's end.
    labels.set('u1', '✎ 3 files');
    await handlers.get('agent_end')!({}, ctx);
    expect(labels.has('u1')).toBe(false);
  });

  it('rewinds a turn that has no anchor, which no branch can place', async () => {
    process.env['OCTOCODE_HOME'] = tmp();
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'one');
    const { handlers, commands, branch, ctx, edit } = host(cwd);
    branch.length = 0;
    await handlers.get('session_start')!({}, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await edit('one', 'two');
    await commands.get('rewind')!.handler('', ctx as never);
    expect(fs.readFileSync(file, 'utf8')).toBe('one');
  });

  it('offers to restore files when /tree navigation leaves turns that changed them', async () => {
    process.env['OCTOCODE_HOME'] = tmp();
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'one');
    const { handlers, branch, selects, ctx, edit } = host(cwd);
    await handlers.get('session_start')!({}, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await edit('one', 'two');
    branch.push({ id: 'a1', type: 'message', message: { role: 'assistant' } }, { id: 'u2', type: 'message', message: { role: 'user' } });
    await handlers.get('input')!({ source: 'interactive', text: 'next' }, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await edit('two', 'three');
    branch.push({ id: 'a2', type: 'message', message: { role: 'assistant' } });
    const tree = (targetId: string) => handlers.get('session_before_tree')!({ type: 'session_before_tree', preparation: { targetId }, signal: new AbortController().signal }, ctx);

    // Selecting the last assistant reply keeps every turn: nothing to offer.
    await tree('a2');
    expect(selects).toEqual([]);
    // Selecting the second user message puts it back in the editor: the leaf becomes a1, so turn 2 is left behind.
    await tree('u2');
    expect(selects).toEqual(['The agent changed 1 file after this point. Restore them?']);
    // A navigation cancelled after the answer (Esc during the summary) never reaches session_tree: files stay.
    expect(fs.readFileSync(file, 'utf8')).toBe('three');
    await tree('u2');
    await handlers.get('session_tree')!({ type: 'session_tree', newLeafId: 'a1', oldLeafId: 'a2' }, ctx);
    expect(fs.readFileSync(file, 'utf8')).toBe('two');
    // Selecting the first user message leaves the root: turn 1 is undone too.
    await tree('u1');
    await handlers.get('session_tree')!({ type: 'session_tree', newLeafId: null, oldLeafId: 'a2' }, ctx);
    expect(fs.readFileSync(file, 'utf8')).toBe('one');
    // A later move restores nothing an earlier, answered offer chose.
    await handlers.get('session_tree')!({ type: 'session_tree', newLeafId: null, oldLeafId: null }, ctx);
    expect(fs.readFileSync(file, 'utf8')).toBe('one');
  });

  it('lets /rewind in a forked session undo turns made before the fork', async () => {
    process.env['OCTOCODE_HOME'] = tmp();
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'one');
    const parent = host(cwd, 'parent/1');
    await parent.handlers.get('session_start')!({}, parent.ctx);
    await parent.handlers.get('agent_start')!({}, parent.ctx);
    await parent.edit('one', 'two');
    const sessionFile = path.join(tmp(), 'parent.jsonl');
    fs.writeFileSync(sessionFile, `${JSON.stringify({ type: 'session', id: 'parent/1' })}\n`);

    // The fork runs in a fresh extension instance (Pi rebuilds the runtime) with the same entry ids on its branch.
    const child = host(cwd, 'child/1');
    child.branch.push({ id: 'a1', type: 'message', message: { role: 'assistant' } });
    await child.handlers.get('session_start')!({ type: 'session_start', reason: 'fork', previousSessionFile: sessionFile }, child.ctx);
    await child.commands.get('rewind')!.handler('', child.ctx as never);
    expect(child.notices.pop()).toMatch(/Rewound 1 turn: restored 1 file/);
    expect(fs.readFileSync(file, 'utf8')).toBe('one');
  });

  it('inherits the parent session checkpoints for a `pi --fork` start, named by the session header', async () => {
    process.env['OCTOCODE_HOME'] = tmp();
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'one');
    const parent = host(cwd, 'parent/2');
    await parent.handlers.get('session_start')!({}, parent.ctx);
    await parent.handlers.get('agent_start')!({}, parent.ctx);
    await parent.edit('one', 'two');
    const sessionFile = path.join(tmp(), 'parent.jsonl');
    fs.writeFileSync(sessionFile, `${JSON.stringify({ type: 'session', id: 'parent/2' })}\n`);

    // `pi --fork <file>` starts the child with reason 'startup'; only its header names the parent.
    const child = host(cwd, 'child/2', { parentSession: sessionFile });
    child.branch.push({ id: 'a1', type: 'message', message: { role: 'assistant' } });
    await child.handlers.get('session_start')!({ type: 'session_start', reason: 'startup' }, child.ctx);
    await child.commands.get('rewind')!.handler('', child.ctx as never);
    expect(child.notices.pop()).toMatch(/Rewound 1 turn: restored 1 file/);
    expect(fs.readFileSync(file, 'utf8')).toBe('one');
  });

  it('keeps a run in one turn when the user steers mid-run; the next prompt opens the next turn', async () => {
    process.env['OCTOCODE_HOME'] = tmp();
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'one');
    const { handlers, commands, notices, ctx, edit } = host(cwd);
    await handlers.get('session_start')!({}, ctx);
    await handlers.get('input')!({ source: 'interactive', text: 'first' }, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await edit('one', 'two');
    // Typed while the run streams: it must not split the running turn.
    await handlers.get('input')!({ source: 'interactive', text: 'also', streamingBehavior: 'steer' }, ctx);
    await edit('two', 'three');
    await handlers.get('input')!({ source: 'interactive', text: 'second' }, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await edit('three', 'four');
    // Two turns: the first run (one → three) and the second prompt (three → four).
    await commands.get('rewind')!.handler('2', ctx as never);
    expect(notices.pop()).toMatch(/Rewound 2 turns/);
    expect(fs.readFileSync(file, 'utf8')).toBe('one');
  });

  it('reads a file at most twice per change: once before it, once for the atomic compare-and-swap', async () => {
    process.env['OCTOCODE_HOME'] = tmp();
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'one');
    const { handlers, ctx, edit, run } = host(cwd);
    await handlers.get('session_start')!({}, ctx);
    await handlers.get('agent_start')!({}, ctx);
    const sync = vi.spyOn(fs, 'readFileSync');
    const promised = vi.spyOn(fs.promises, 'readFile');
    const real = fs.realpathSync(file);
    const reads = () => [...sync.mock.calls, ...promised.mock.calls].filter(([target]) => [file, real].includes(String(target))).length;
    try {
      await edit('one', 'two');
      expect(reads()).toBe(2);
      sync.mockClear();
      promised.mockClear();
      await run([{ reasoning: 'r', type: 'write', path: 'a.txt', content: 'three' }]);
      expect(reads()).toBe(2);
      sync.mockClear();
      promised.mockClear();
      await run([{ reasoning: 'r', type: 'delete', path: 'a.txt' }]);
      expect(reads()).toBe(2);
    } finally {
      sync.mockRestore();
      promised.mockRestore();
    }
    expect(fs.existsSync(file)).toBe(false);
  });
});
