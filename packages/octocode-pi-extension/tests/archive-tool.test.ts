import fs from 'node:fs';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { archivePointer, forkLineage, sessionRecordOf } from '../src/compaction/archive.js';
import { registerArchiveTool } from '../src/compaction/archive-tool.js';
import { registerCompaction } from '../src/compaction/register.js';
import { FileGuard } from '../src/files/guard.js';
import { archiveFolder, ownArchives, RECORD_FILE, sessionLineage, setSessionLineage, writeSegment, type Segment } from '../src/shared/archive.js';
import { readRecordHeader, writeSessionRecord } from '../src/shared/session-record.js';
import { PARENT_SESSION_ENV } from '../src/shared/env.js';
import { setCurrentSession } from '../src/shared/home.js';
import { fakeCtx, fakePi } from './fake-pi.js';
import { tmp } from './helpers.js';

function repo(): string {
  const cwd = tmp();
  fs.mkdirSync(path.join(cwd, '.git'));
  return cwd;
}

const segment = (session: string): Omit<Segment, 'n'> => ({ session, entry: `e-${session}`, tokensBefore: 1_000, method: 'cache-shared', tldr: `work of ${session}`, topics: [], readFiles: [], modifiedFiles: [], messages: 1 });

function fileFor(id: string, body: string, cwd?: string): void {
  setCurrentSession(id);
  writeSegment(segment(id), body);
  if (cwd) writeSessionRecord({ session: id, repository: fs.realpathSync(cwd), workspace: cwd, goal: { role: 'user', text: `goal of ${id}` }, ending: [], toolCalls: 0 });
  setCurrentSession('me');
}

let saved: string | undefined;
beforeEach(() => {
  saved = process.env['OCTOCODE_HOME'];
  process.env['OCTOCODE_HOME'] = tmp();
  setCurrentSession('me');
});
afterEach(() => {
  setCurrentSession(undefined);
  setSessionLineage([]);
  vi.unstubAllEnvs();
  if (saved === undefined) delete process.env['OCTOCODE_HOME'];
  else process.env['OCTOCODE_HOME'] = saved;
});

function tool(cwd: string) {
  const fake = fakePi();
  registerArchiveTool(fake.pi);
  const archive = fake.tools.get('archive');
  const ctx = fakeCtx({ cwd });
  return async (params: Record<string, unknown>) => {
    const result = await archive.execute('t1', params, undefined, undefined, ctx);
    return { text: result.content[0].text as string, details: result.details };
  };
}

const why = { goal: 'find the login fix', reason: 'the user asked what was decided last week' };

describe('the archive tool', () => {
  it('needs a goal and a reason, and records both with the result', async () => {
    const call = tool(repo());
    await expect(call({ goal: ' ', reason: 'x' })).rejects.toThrow(/needs goal .* and reason/);
    await expect(call({ goal: 'x' })).rejects.toThrow(/needs goal .* and reason/);
    const empty = await call(why);
    expect(empty.text).toMatch(/^No history yet/);
    expect(empty.details).toMatchObject({ ...why, mode: 'list' });
  });

  it('lists, searches with a continuation, and reads from a hit\'s line', async () => {
    const cwd = repo();
    fileFor('me', ['intro', 'the login fix was a redirect', ...Array.from({ length: 200 }, (_, index) => `filler ${index}`)].join('\n'), cwd);
    fileFor('sib', 'login also failed here', cwd);
    const call = tool(cwd);
    const listed = await call(why);
    expect(listed.text).toMatch(/^2 sessions with history, nearest first/);
    expect(listed.text).toContain('## me (this session)');
    expect(listed.text).toContain('## sib (same repository)');
    const found = await call({ ...why, query: 'LOGIN' });
    expect(found.text).toMatch(/^2 matching lines in 2 of 4 files \(2 sessions\)/);
    expect(found.details).toMatchObject({ mode: 'search', hits: 2, files: 2, page: 1, pages: 1 });
    const line = Number(/^>\s*(\d+)│ the login fix/m.exec(found.text)![1]);
    // A bare file name is a file of this session's archive, as the pointer names them.
    const read = await call({ ...why, file: '1-e-me.md', line });
    expect(read.text).toContain('the login fix was a redirect');
    expect(read.text).toMatch(/Lines \d+–\d+ of \d+; next: line \d+\.$/);
    const narrowed = await call({ ...why, query: 'login', file: archiveFolder('sib') });
    expect(narrowed.details).toMatchObject({ hits: 1, files: 1 });
  });

  it('pages a long search and says how to continue', async () => {
    const cwd = repo();
    fileFor('me', Array.from({ length: 3_000 }, (_, index) => `match ${index} ${'z'.repeat(30)}`).join('\n'), cwd);
    const call = tool(cwd);
    const first = await call({ ...why, query: 'match' });
    expect(first.text).toMatch(/Page 1 of \d+; next: page 2 with the same query and file\.$/);
    expect(first.text).toMatch(/^3000 matching lines in 1 of 2 files \(1 session\)/);
    expect((await call(why)).text).toMatch(/^1 session with history/);
    expect((await call({ ...why, query: 'nothing-like-this' })).text).toContain('in 2 files of 1 session.');
    const last = await call({ ...why, query: 'match', page: first.details.pages });
    expect(last.text).toMatch(/Page \d+ of \d+, the last\.$/);
  });

  it('refuses another repository\'s history and says why; an invalid query names the fix', async () => {
    const cwd = repo();
    fileFor('far', 'secret', repo());
    const call = tool(cwd);
    await expect(call({ ...why, file: path.join(archiveFolder('far'), RECORD_FILE) })).rejects.toThrow(/not in the history this session may read.*needs `read`, which asks the user/);
    await expect(call({ ...why, query: 'a(' })).rejects.toThrow(/invalid .*escape/);
    const missed = await call({ ...why, query: 'nothing-like-this' });
    expect(missed.text).toMatch(/^No line matches .* Try another term or spelling/);
  });
});

/** A Pi session file whose first line is its header. */
function sessionFile(id: string, parentSession?: string): string {
  const file = path.join(tmp(), `${id}.jsonl`);
  fs.writeFileSync(file, `${JSON.stringify({ type: 'session', version: 3, id, timestamp: '2026-01-01T00:00:00Z', cwd: '/r', ...(parentSession ? { parentSession } : {}) })}\n{"type":"message"}\n`);
  return file;
}

describe('fork lineage (B155)', () => {
  it('follows each session file back to the first, nearest first, and stops at a loop or a missing file', () => {
    const root = sessionFile('root');
    const middle = sessionFile('middle', root);
    expect(forkLineage({ getHeader: () => ({ type: 'session', id: 'me', timestamp: '', cwd: '/r', parentSession: middle }) })).toEqual(['middle', 'root']);
    expect(forkLineage({ getHeader: () => ({ type: 'session', id: 'me', timestamp: '', cwd: '/r', parentSession: path.join(tmp(), 'gone.jsonl') }) })).toEqual([]);
    const loop = path.join(tmp(), 'loop.jsonl');
    fs.writeFileSync(loop, `${JSON.stringify({ type: 'session', id: 'loop', timestamp: '', cwd: '/r', parentSession: loop })}\n`);
    expect(forkLineage({ getHeader: () => ({ type: 'session', id: 'me', timestamp: '', cwd: '/r', parentSession: loop }) })).toEqual(['loop']);
    expect(forkLineage({ getHeader: () => null })).toEqual([]);
  });

  it('sets the lineage at session start, so the pointer names the source\'s archive and it reads as the session\'s own', async () => {
    fileFor('source', 'before the fork');
    const fake = fakePi();
    registerCompaction(fake.pi, new FileGuard());
    const ctx = fakeCtx({ cwd: repo() });
    Object.assign(ctx.sessionManager, { getHeader: () => ({ type: 'session', id: 'me', timestamp: '', cwd: '/r', parentSession: sessionFile('source') }) });
    await fake.fire('session_start', { type: 'session_start', reason: 'fork' }, ctx);
    expect(sessionLineage()).toEqual(['source']);
    expect(ownArchives()).toContain(archiveFolder('source'));
    expect(archivePointer(1).at(-1)).toBe(`Forked from session source: its history before the fork, and the segments notes named before it, are in ${archiveFolder('source')} (1 segments).`);
  });
});

describe('session record', () => {
  const entry = (id: string, role: string, content: unknown, timestamp: string) => ({ type: 'message', id, parentId: null, timestamp, message: { role, content, timestamp: 0 } });
  const branch = [
    entry('1', 'user', 'Fix the login redirect\nwith details', '2026-01-01T00:00:00Z'),
    entry('2', 'assistant', [{ type: 'text', text: 'On it.' }], '2026-01-01T00:01:00Z'),
    entry('3', 'user', [{ type: 'text', text: 'Now ship it' }], '2026-01-01T00:02:00Z'),
    entry('4', 'assistant', [{ type: 'thinking', thinking: 'hmm' }, { type: 'toolCall', id: 'c1', name: 'bash', arguments: {} }], '2026-01-01T00:03:00Z'),
    entry('5', 'toolResult', [{ type: 'text', text: 'ok' }], '2026-01-01T00:04:00Z'),
    entry('6', 'assistant', [{ type: 'text', text: 'Shipped in v2.' }], '2026-01-01T00:05:00Z'),
  ];
  const manager = (cwd: string, file: string | undefined) => ({
    getSessionFile: () => file, getBranch: () => branch, getSessionId: () => 'me', getCwd: () => cwd, getSessionName: () => 'login work',
    getHeader: () => ({ type: 'session', id: 'me', timestamp: '2026-01-01T00:00:00Z', cwd }),
  });

  it('keeps the first user message as the goal and the last one with the replies after it as the ending', () => {
    const cwd = repo();
    vi.stubEnv(PARENT_SESSION_ENV, 'boss');
    const made = sessionRecordOf(manager(cwd, '/t/me.jsonl') as never, ['source'])!;
    expect(made).toMatchObject({
      session: 'me', repository: fs.realpathSync(cwd), workspace: cwd, forkedFrom: 'source', parent: 'boss', name: 'login work',
      started: '2026-01-01T00:00:00Z', lastActive: '2026-01-01T00:05:00Z', transcript: '/t/me.jsonl',
      goal: { role: 'user', text: 'Fix the login redirect\nwith details' }, toolCalls: 1,
    });
    expect(made.ending.map((message) => message.text)).toEqual(['Now ship it', 'Shipped in v2.']);
    writeSessionRecord(made);
    expect(readRecordHeader(archiveFolder())).toMatchObject({ session: 'me', forkedFrom: 'source', goal: 'Fix the login redirect', ended: 'Shipped in v2.' });
    expect(fs.readFileSync(path.join(archiveFolder(), RECORD_FILE), 'utf8')).toContain('its 1 tool calls are in the segments and the transcript');
    // A session that is not saved leaves nothing behind.
    expect(sessionRecordOf(manager(cwd, undefined) as never, [])).toBeUndefined();
  });

  it('is written after each run', async () => {
    const cwd = repo();
    const fake = fakePi();
    registerCompaction(fake.pi, new FileGuard());
    const ctx = fakeCtx({ cwd });
    Object.assign(ctx.sessionManager, manager(cwd, '/t/me.jsonl'));
    await fake.fire('agent_end', { type: 'agent_end', messages: [] }, ctx);
    expect(readRecordHeader(archiveFolder())).toMatchObject({ session: 'me', ended: 'Shipped in v2.' });
  });
});
