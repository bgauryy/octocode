import fs from 'node:fs';
import path from 'node:path';
import type { ExtensionCommandContext, SessionEntry } from '@earendil-works/pi-coding-agent';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Checkpoints } from '../src/files/checkpoint.js';
import { registerCheckpoints } from '../src/files/checkpoint-command.js';
import { FileGuard } from '../src/files/guard.js';
import { registerFileTool } from '../src/files/tool.js';
import { sha256 } from '../src/shared/atomic.js';
import { Subcommands } from '../src/shared/commands.js';
import { sessionDir } from '../src/shared/home.js';
import { messageEntry, stub } from './doubles.js';
import { fakePermission, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

/** A fake Pi host with the file tool, checkpoints and a branch the test extends. */
function host(cwd: string, sessionId = 'session/1', header?: { parentSession?: string }) {
  const fake = fakePi();
  const commands = new Subcommands();
  const guard = new FileGuard();
  // These tests are about checkpoints: the agent has read the workspace files it changes (the guard requires it).
  for (const name of fs.readdirSync(cwd)) guard.record(path.join(cwd, name));
  const checkpoints = new Checkpoints();
  registerFileTool(fake.pi, guard, fakePermission(), checkpoints);
  const forgotten: string[] = [];
  registerCheckpoints(fake.pi, checkpoints, (target) => forgotten.push(target), commands);
  // The file tool and checkpoints register at most one handler per event: each test calls that one directly.
  const handlers = new Map([...fake.handlers].map(([name, [handler, ...more]]) => {
    if (!handler || more.length > 0) throw new Error(`expected one ${name} handler`);
    return [name, handler];
  }));
  const tool = fake.tools.get('file');
  const { sent, labels } = fake;
  const branch: SessionEntry[] = [messageEntry('u1', 'user')];
  const notices: string[] = [];
  const selects: string[] = [];
  const ctx = stub<ExtensionCommandContext>({
    cwd,
    hasUI: true,
    sessionManager: stub<ExtensionCommandContext['sessionManager']>({
      getSessionId: () => sessionId,
      getHeader: () => (header ? { type: 'session', id: sessionId, timestamp: '', cwd, ...header } : null),
      // The branch is one path: each entry's parent is the one before it.
      getBranch: (from?: string) => (from === undefined ? branch : branch.slice(0, branch.findIndex((entry) => entry.id === from) + 1)),
      getEntry: (id: string) => {
        const index = branch.findIndex((entry) => entry.id === id);
        return index < 0 ? undefined : { ...branch[index], parentId: branch[index - 1]?.id ?? null };
      },
      getLabel: (id: string) => labels.get(id),
    }),
    ui: stub<ExtensionCommandContext['ui']>({ notify: (text: string) => notices.push(text), select: async (title: string, options: string[]) => (selects.push(title), options[0]) }),
  });
  const run = (queries: unknown[]) => tool.execute('t', { queries }, undefined, undefined, ctx);
  const edit = (from: string, to: string) => run([{ reasoning: 'r', type: 'edit', path: 'a.txt', edits: [{ oldText: from, newText: to }] }]);
  return { handlers, commands, guard, forgotten, branch, notices, selects, sent, ctx, run, edit, labels };
}

const home = process.env['OCTOCODE_HOME'];
afterEach(() => {
  if (home === undefined) delete process.env['OCTOCODE_HOME'];
  else process.env['OCTOCODE_HOME'] = home;
});

describe('/rewind', () => {
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
    branch.push(messageEntry('a1', 'assistant'), messageEntry('u2', 'user'));
    // A run woken by a team message or job report is not a turn of its own; the user's next prompt is.
    await handlers.get('input')!({ source: 'extension', text: 'report' }, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await handlers.get('input')!({ source: 'interactive', text: 'next' }, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await edit('two', 'three');
    expect(fs.existsSync(path.join(process.env['OCTOCODE_HOME'], 'agent', 'pi', 'sessions', 'session_1', 'checkpoints', 'journal.jsonl'))).toBe(true);

    await commands.get('rewind')!.handler('', ctx);
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
    await commands.get('rewind')!.handler('3', ctx);
    expect(notices.pop()).toBe('No file changes to rewind on this branch.');
    expect(sent).toHaveLength(1);
  });

  it('reports a file restored only part of the way, tells the model, and keeps the rest to retry', async () => {
    process.env['OCTOCODE_HOME'] = tmp();
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'one');
    const { handlers, commands, forgotten, branch, notices, sent, ctx, edit } = host(cwd);
    await handlers.get('session_start')!({}, ctx);
    const turn = async (from: string, to: string, id: number) => {
      branch.push(messageEntry(`a${id}`, 'assistant'), messageEntry(`u${id}`, 'user'));
      await handlers.get('input')!({ source: 'interactive', text: 'next' }, ctx);
      await handlers.get('agent_start')!({}, ctx);
      await edit(from, to);
    };
    await handlers.get('agent_start')!({}, ctx);
    await edit('one', 'two');
    await turn('two', 'three', 2);
    await turn('three', 'four', 3);
    const blobs = path.join(process.env['OCTOCODE_HOME'], 'agent', 'pi', 'sessions', 'session_1', 'checkpoints', 'blobs');
    fs.writeFileSync(path.join(blobs, sha256('two')), 'evil');
    await commands.get('rewind')!.handler('3', ctx);
    expect(fs.readFileSync(file, 'utf8')).toBe('three');
    expect(notices.pop()).toMatch(/restored 0 files\.\nPartly restored[^\n]*: a\.txt\./);
    expect(forgotten).toEqual([file]);
    expect(sent).toHaveLength(1);
    // The older checkpoints stayed: once the blob is sound, rewinding again finishes the job.
    fs.writeFileSync(path.join(blobs, sha256('two')), 'two');
    await commands.get('rewind')!.handler('3', ctx);
    expect(fs.readFileSync(file, 'utf8')).toBe('one');
    expect(notices.pop()).toMatch(/restored 1 file \(a\.txt\)/);
  });
});

describe('/rewind on the session branch', () => {
  it('rewinds only turns on the current branch after a /tree move', async () => {
    process.env['OCTOCODE_HOME'] = tmp();
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'one');
    const { handlers, commands, branch, notices, ctx, edit } = host(cwd);
    await handlers.get('session_start')!({}, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await edit('one', 'two');
    branch.push(messageEntry('a1', 'assistant'), messageEntry('u2', 'user'));
    await handlers.get('input')!({ source: 'interactive', text: 'next' }, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await edit('two', 'three');
    // The user moves back to a1 and starts another branch (u3) without restoring: turn 2 (anchored at u2) is off-branch.
    branch.splice(2, 1, messageEntry('u3', 'user'));
    await commands.get('rewind')!.handler('', ctx);
    expect(notices.pop()).toMatch(/Rewound 1 turn/);
    // Turn 1 (u1) was rewound; turn 2's change from another branch is left: the file had moved on, so it is skipped.
    expect(fs.readFileSync(file, 'utf8')).toBe('three');
    // The skipped checkpoint stays, so a second rewind retries it (the file is still not as the agent left it).
    await commands.get('rewind')!.handler('', ctx);
    expect(notices.pop()).toMatch(/Left alone[\s\S]*a\.txt/);
    expect(fs.readFileSync(file, 'utf8')).toBe('three');
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
    branch.push(messageEntry('a1', 'assistant'), messageEntry('u2', 'user'));
    labels.set('u2', '✎ mine');
    await handlers.get('input')!({ source: 'interactive', text: 'next' }, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await run([{ reasoning: 'r', type: 'write', path: 'a.txt', content: 'three' }]);
    await handlers.get('agent_end')!({}, ctx);
    // A user label that merely starts with the mark is still the user's.
    expect(labels.get('u2')).toBe('✎ mine');
    await commands.get('rewind')!.handler('2', ctx);
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
    await commands.get('rewind')!.handler('', ctx);
    expect(fs.readFileSync(file, 'utf8')).toBe('one');
  });
});

describe('the fork offer', () => {
  it('offers to restore files when /tree navigation leaves turns that changed them', async () => {
    process.env['OCTOCODE_HOME'] = tmp();
    const cwd = tmp();
    const file = path.join(cwd, 'a.txt');
    fs.writeFileSync(file, 'one');
    const { handlers, branch, selects, ctx, edit } = host(cwd);
    await handlers.get('session_start')!({}, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await edit('one', 'two');
    branch.push(messageEntry('a1', 'assistant'), messageEntry('u2', 'user'));
    await handlers.get('input')!({ source: 'interactive', text: 'next' }, ctx);
    await handlers.get('agent_start')!({}, ctx);
    await edit('two', 'three');
    branch.push(messageEntry('a2', 'assistant'));
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
    child.branch.push(messageEntry('a1', 'assistant'));
    await child.handlers.get('session_start')!({ type: 'session_start', reason: 'fork', previousSessionFile: sessionFile }, child.ctx);
    await child.commands.get('rewind')!.handler('', child.ctx);
    expect(child.notices.pop()).toMatch(/Rewound 1 turn: restored 1 file/);
    expect(fs.readFileSync(file, 'utf8')).toBe('one');
  });
});

describe('checkpoints inherited by a fork', () => {
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
    child.branch.push(messageEntry('a1', 'assistant'));
    await child.handlers.get('session_start')!({ type: 'session_start', reason: 'startup' }, child.ctx);
    await child.commands.get('rewind')!.handler('', child.ctx);
    expect(child.notices.pop()).toMatch(/Rewound 1 turn: restored 1 file/);
    expect(fs.readFileSync(file, 'utf8')).toBe('one');
  });

  it('tells the user when a fork could not inherit the parent checkpoints', async () => {
    process.env['OCTOCODE_HOME'] = tmp();
    const cwd = tmp();
    fs.writeFileSync(path.join(cwd, 'a.txt'), 'one');
    const parent = host(cwd, 'parent/3');
    await parent.handlers.get('session_start')!({}, parent.ctx);
    await parent.handlers.get('agent_start')!({}, parent.ctx);
    await parent.edit('one', 'two');
    const sessionFile = path.join(tmp(), 'parent.jsonl');
    fs.writeFileSync(sessionFile, `${JSON.stringify({ type: 'session', id: 'parent/3' })}\n`);
    // A file where the fork's checkpoint folder belongs: the seed cannot be written.
    fs.mkdirSync(sessionDir('child/3'), { recursive: true });
    fs.writeFileSync(path.join(sessionDir('child/3'), 'checkpoints'), 'in the way');
    const child = host(cwd, 'child/3', { parentSession: sessionFile });
    child.branch.push(messageEntry('a1', 'assistant'));
    await child.handlers.get('session_start')!({ type: 'session_start', reason: 'startup' }, child.ctx);
    expect(child.notices).toEqual([expect.stringMatching(/could not carry the checkpoints to this fork \(.+\); \/octocode rewind will not reach the parent's turns/)]);
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
    await commands.get('rewind')!.handler('2', ctx);
    expect(notices.pop()).toMatch(/Rewound 2 turns/);
    expect(fs.readFileSync(file, 'utf8')).toBe('one');
  });
});

describe('checkpoint reads', () => {
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
    // The compare-and-swap streams its hash.
    const streamed = vi.spyOn(fs, 'createReadStream');
    const real = fs.realpathSync(file);
    const reads = () => [...sync.mock.calls, ...promised.mock.calls, ...streamed.mock.calls].filter(([target]) => (typeof target === 'string' || Buffer.isBuffer(target)) && [file, real].includes(target.toString())).length;
    try {
      await edit('one', 'two');
      expect(reads()).toBe(2);
      sync.mockClear();
      promised.mockClear();
      streamed.mockClear();
      await run([{ reasoning: 'r', type: 'write', path: 'a.txt', content: 'three' }]);
      expect(reads()).toBe(2);
      sync.mockClear();
      promised.mockClear();
      streamed.mockClear();
      await run([{ reasoning: 'r', type: 'delete', path: 'a.txt' }]);
      expect(reads()).toBe(2);
    } finally {
      sync.mockRestore();
      promised.mockRestore();
      streamed.mockRestore();
    }
    expect(fs.existsSync(file)).toBe(false);
  });
});
